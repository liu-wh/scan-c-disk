use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

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
}

impl Node {
    fn new(name: String, value: Option<u64>, children: Option<Vec<Node>>) -> Self {
        Self { name, size: value.map(human_readable), value, children }
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

async fn full_size(path: &Path) -> u64 {
    let mut rd = match fs::read_dir(path).await { Ok(rd) => rd, Err(_) => return 0 };
    let mut files_total: u64 = 0;
    let mut subdirs: Vec<std::path::PathBuf> = Vec::new();
    while let Ok(Some(entry)) = rd.next_entry().await {
        let ft = match entry.file_type().await { Ok(ft) => ft, Err(_) => continue };
        if ft.is_symlink() { continue; }
        let p = entry.path();
        if ft.is_dir() { if !is_skipped(&p) { subdirs.push(p); } }
        else if ft.is_file() { if let Ok(meta) = entry.metadata().await { files_total += meta.len(); } }
    }
    let sums = join_all(subdirs.iter().map(|d| full_size(d))).await;
    files_total + sums.into_iter().sum::<u64>()
}

async fn build_node_with_depth(path: &Path, level: usize, max_depth: usize) -> Node {
    let name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    if level >= max_depth {
        return Node::new(name, Some(full_size(path).await), None);
    }
    let mut rd = match fs::read_dir(path).await {
        Ok(rd) => rd,
        Err(_) => return Node::new(name, Some(full_size(path).await), None),
    };
    let mut loose_files: u64 = 0;
    let mut subdirs: Vec<std::path::PathBuf> = Vec::new();
    while let Ok(Some(entry)) = rd.next_entry().await {
        let ft = match entry.file_type().await { Ok(ft) => ft, Err(_) => continue };
        if ft.is_symlink() { continue; }
        let p = entry.path();
        if ft.is_dir() { if !is_skipped(&p) { subdirs.push(p); } }
        else if ft.is_file() { if let Ok(meta) = entry.metadata().await { loose_files += meta.len(); } }
    }
    let mut children: Vec<Node> = join_all(
        subdirs.iter().map(|d| build_node_with_depth(d, level + 1, max_depth))
    ).await;
    children.sort_by(|a, b| b.value.unwrap_or(0).cmp(&a.value.unwrap_or(0)));
    let total = loose_files + children.iter().map(|c| c.value.unwrap_or(0)).sum::<u64>();
    Node::new(name, Some(total), if children.is_empty() { None } else { Some(children) })
}

// ✅ scan_disk 只在这里定义一次
#[tauri::command]
pub async fn scan_disk(request: ScanRequest) -> Result<Node, String> {
    let root_path = Path::new(&request.root);
    if !root_path.exists() { return Err(format!("路径不存在: {}", request.root)); }
    if !root_path.is_dir() { return Err(format!("不是目录: {}", request.root)); }
    Ok(build_node_with_depth(root_path, 1, request.max_depth).await)
}