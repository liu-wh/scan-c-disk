use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

// 并行 worker 数：取逻辑核数，上限 16（实测 >16 收益递减、易过度订阅）。
fn size_workers() -> usize {
    thread::available_parallelism().map(|n| n.get()).unwrap_or(8).min(16)
}

fn human_readable(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes == 0 { return "0 B".to_string(); }
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if size.fract() == 0.0 {
        format!("{:.0} {}", size, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

#[derive(Serialize)]
pub struct Node {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Node>>,
    #[serde(skip)]
    total_size: u64,
}

impl Node {
    fn root(name: String, size: u64, children: Option<Vec<Node>>) -> Self {
        Self { name, size: Some(human_readable(size)), value: None, children, total_size: size }
    }

    fn without_size(
        name: String,
        value: Option<u64>,
        children: Option<Vec<Node>>,
        total_size: u64,
    ) -> Self {
        Self { name, size: None, value, children, total_size }
    }
}

#[derive(Deserialize)]
pub struct ScanRequest {
    #[serde(default = "default_root")]
    pub root: String,
    #[serde(default = "default_depth")]
    pub max_depth: usize,
}
fn default_root() -> String { "C:\\".to_string() }
fn default_depth() -> usize { 5 }

fn is_skipped(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    let s = s.trim_end_matches('\\');
    matches!(s, "c:\\windows\\winsxs" | "c:\\programdata\\microsoft\\windows\\wer")
}

// 浅层目录的扫描骨架：只展开 level < max_depth 的目录，
// 达到 max_depth 的目录（以及无法读取、需要回退全量统计的目录）记为叶子，稍后统一并行统计。
struct Partial {
    name: String,
    path: PathBuf,
    needs_full_size: bool,
    loose_files: u64,
    children: Vec<Partial>,
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn collect_partial(path: &Path, level: usize, max_depth: usize) -> Partial {
    if level >= max_depth {
        return Partial { name: display_name(path), path: path.to_path_buf(), needs_full_size: true, loose_files: 0, children: Vec::new() };
    }
    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => {
            return Partial { name: display_name(path), path: path.to_path_buf(), needs_full_size: true, loose_files: 0, children: Vec::new() };
        }
    };
    let mut loose_files = 0u64;
    let mut children: Vec<Partial> = Vec::new();
    for entry in rd {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        let ft = match entry.file_type() { Ok(ft) => ft, Err(_) => continue };
        if ft.is_symlink() { continue; }
        let p = entry.path();
        if ft.is_dir() {
            if !is_skipped(&p) { children.push(collect_partial(&p, level + 1, max_depth)); }
        } else if ft.is_file() {
            if let Ok(meta) = entry.metadata() { loose_files += meta.len(); }
        }
    }
    Partial { name: display_name(path), path: path.to_path_buf(), needs_full_size: false, loose_files, children }
}

// 迭代版全量统计：显式栈，避免深层目录的递归开销；跳过符号链接/junction。
fn full_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = match fs::read_dir(&dir) { Ok(rd) => rd, Err(_) => continue };
        for entry in rd {
            let entry = match entry { Ok(e) => e, Err(_) => continue };
            let ft = match entry.file_type() { Ok(ft) => ft, Err(_) => continue };
            if ft.is_symlink() { continue; }
            if ft.is_dir() {
                if !is_skipped(&entry.path()) { stack.push(entry.path()); }
            } else if ft.is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

// 有界并行：每个叶子目录是一个任务，rayon work-stealing 动态负载均衡。
// 相比静态等分切片，可消除"大目录拖后腿"的木桶效应（实测 ~1.3-1.4x）。
fn size_leaves_parallel(paths: &[PathBuf]) -> HashMap<PathBuf, u64> {
    if paths.is_empty() { return HashMap::new(); }
    let n = paths.len().min(size_workers()).max(1);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(n).build().unwrap();
    pool.install(|| {
        use rayon::prelude::*;
        paths.par_iter()
            .map(|p| (p.clone(), full_size(p)))
            .collect()
    })
}

fn gather_leaves(partial: &Partial, out: &mut Vec<PathBuf>) {
    if partial.needs_full_size {
        out.push(partial.path.clone());
    }
    for c in &partial.children {
        gather_leaves(c, out);
    }
}

fn assemble(partial: Partial, level: usize, sizes: &HashMap<PathBuf, u64>) -> Node {
    let name = partial.name;
    if partial.needs_full_size {
        let value = Some(sizes.get(&partial.path).copied().unwrap_or(0));
        return if level == 1 {
            Node::root(name, value.unwrap_or(0), None)
        } else {
            let total = value.unwrap_or(0);
            Node::without_size(name, value, None, total)
        };
    }
    let mut children: Vec<Node> = partial.children.into_iter().map(|c| assemble(c, level + 1, sizes)).collect();
    children.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    let total = partial.loose_files + children.iter().map(|c| c.total_size).sum::<u64>();
    if children.is_empty() {
        if level == 1 {
            Node::root(name, total, None)
        } else {
            Node::without_size(name, Some(total), None, total)
        }
    } else {
        if level == 1 {
            Node::root(name, total, Some(children))
        } else {
            Node::without_size(name, None, Some(children), total)
        }
    }
}

fn run_scan(root: &Path, max_depth: usize) -> Node {
    let partial = collect_partial(root, 1, max_depth);
    let mut leaves: Vec<PathBuf> = Vec::new();
    gather_leaves(&partial, &mut leaves);
    let sizes = size_leaves_parallel(&leaves);
    assemble(partial, 1, &sizes)
}

#[tauri::command]
pub async fn scan_disk(request: ScanRequest) -> Result<Node, String> {
    let root_path = Path::new(&request.root).to_path_buf();
    if !root_path.exists() { return Err(format!("路径不存在: {}", request.root)); }
    if !root_path.is_dir() { return Err(format!("不是目录: {}", request.root)); }
    // 阻塞式扫描放到独立线程，避免占用 tauri 异步运行时；内部再用有界线程池并行统计叶子目录。
    let max_depth = request.max_depth;
    let result = tokio::task::spawn_blocking(move || run_scan(&root_path, max_depth))
        .await
        .map_err(|e| format!("扫描任务异常: {}", e))?;
    Ok(result)
}
