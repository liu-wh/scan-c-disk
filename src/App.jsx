import { useMemo, useState } from "react";
import { Treemap } from "@ant-design/plots";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

const scanRequest = { root: "C:\\", max_depth: 5 };

function formatSize(bytes = 0) {
  const normalizedBytes = Number.isFinite(Number(bytes)) ? Number(bytes) : 0;
  if (normalizedBytes < 1024) return `${normalizedBytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = normalizedBytes;
  let unit = -1;
  do {
    value /= 1024;
    unit += 1;
  } while (value >= 1024 && unit < units.length - 1);

  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

function normalizeNode(node, fallbackName = "未命名") {
  if (!node || typeof node !== "object") {
    return { name: fallbackName, value: 0 };
  }

  const value = Number(node.value);
  const children = Array.isArray(node.children)
    ? node.children.map((child) => normalizeNode(child, "未命名"))
    : [];

  return {
    ...node,
    name: typeof node.name === "string" && node.name.length > 0 ? node.name : fallbackName,
    value: Number.isFinite(value) && value >= 0 ? value : 0,
    ...(children.length > 0 ? { children } : {}),
  };
}

function uniqueTreeNodeNames(node) {
  if (!node) return node;

  const nameOccurrences = new Map();
  const children = Array.isArray(node.children)
    ? node.children.map((child) => {
        const name = String(child.name ?? "").trim() || "未命名";
        const occurrence = nameOccurrences.get(name) || 0;
        nameOccurrences.set(name, occurrence + 1);

        return uniqueTreeNodeNames({
          ...child,
          name: occurrence ? `${name}\u2063${occurrence}` : name,
        });
      })
    : undefined;

  return children ? { ...node, children } : { ...node };
}

function displayName(name) {
  return String(name || "-").replace(/\u2063\d+$/, "");
}

function getStats(node) {
  const children = Array.isArray(node?.children) ? node.children : [];
  return children.reduce(
    (stats, child) => {
      const childStats = getStats(child);
      return {
        folders: stats.folders + 1 + childStats.folders,
        files: stats.files + childStats.files,
      };
    },
    { folders: 0, files: 0 },
  );
}

function DiskIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z" />
      <path d="M6 8h12M7 16h.01M11 16h.01" />
    </svg>
  );
}

function ScanIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="10.8" cy="10.8" r="6.2" />
      <path d="m16 16 4.5 4.5M10.8 7.5v6.6M7.5 10.8h6.6" />
    </svg>
  );
}

function App() {
  const [status, setStatus] = useState("idle");
  const [data, setData] = useState(null);
  const [error, setError] = useState("");

  async function startScan() {
    setStatus("scanning");
    setError("");
    try {
      const result = await invoke("scan_disk", { request: scanRequest });
      setData(uniqueTreeNodeNames(normalizeNode(result, scanRequest.root)));
      setStatus("complete");
    } catch (scanError) {
      setError(scanError instanceof Error ? scanError.message : String(scanError));
      setStatus("error");
    }
  }
  const stats = useMemo(() => (data ? getStats(data) : null), [data]);
  const chartConfig = useMemo(() => ({
    encode: { value: "value" },
    interaction: { treemapDrillDown: { breadCrumbY: 12, activeFill: "#873bf4" } },
    legend: { color: { position: "bottom" } },
    tooltip: {
      title: (datum) => displayName(datum?.path?.[datum.path.length - 1] || datum?.name),
      items: [(datum) => {
        const value = Number(datum?.value) || 0;
        const parentValue = Number(datum?.parent?.value) || 0;
        const percentage = parentValue ? ((value / parentValue) * 100).toFixed(2) : "0.00";

        return {
          name: "占用空间",
          value: `${value.toLocaleString()} (${percentage}%)`,
        };
      }],
    },
  }), []);
  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark"><DiskIcon /></span>
          <span>磁盘管家</span>
        </div>
        <span className="topbar-caption">Windows 磁盘空间分析工具</span>
      </header>

      <section className={`hero ${status === "complete" ? "hero-compact" : ""}`}>
        <div className="hero-copy">
          <span className="eyebrow">DISK INSIGHT</span>
          <h1>{status === "complete" ? "C 盘空间概览" : "看清每一份空间的去向"}</h1>
          <p>
            {status === "complete"
              ? "通过可视化图表快速定位占用空间较大的目录。点击色块可以深入查看。"
              : "扫描 C 盘目录并生成直观的空间分布图，帮助你轻松管理存储空间。"}
          </p>
          {status !== "complete" && (
            <button className="scan-button" onClick={startScan} disabled={status === "scanning"}>
              <ScanIcon />
              {status === "scanning" ? "正在扫描 C 盘..." : "开始扫描 C 盘"}
            </button>
          )}
          {status === "error" && <p className="error-message">{error || "扫描失败，请稍后重试。"}</p>}
        </div>
        {status !== "complete" && (
          <div className="hero-art" aria-hidden="true">
            <div className="art-orbit orbit-one" />
            <div className="art-orbit orbit-two" />
            <div className="art-disk"><DiskIcon /></div>
            <span className="art-dot dot-one" />
            <span className="art-dot dot-two" />
            <span className="art-dot dot-three" />
          </div>
        )}
      </section>

      {status === "scanning" && (
        <section className="loading-card">
          <div className="loader" />
          <div>
            <h2>正在分析文件结构</h2>
            <p>这可能需要一些时间，请不要关闭应用...</p>
          </div>
        </section>
      )}

      {status === "complete" && data && (
        <section className="results-section">
          <div className="stats-row">
            <div className="stat-card">
              <span className="stat-label">已扫描位置</span>
              <strong>{scanRequest.root}</strong>
            </div>
            <div className="stat-card">
              <span className="stat-label">占用空间</span>
              <strong>{formatSize(data.value)}</strong>
            </div>
            <div className="stat-card">
              <span className="stat-label">目录数量</span>
              <strong>{stats.folders.toLocaleString()}</strong>
            </div>
          </div>
          <div className="chart-card">
            <div className="card-heading">
              <div>
                <h2>空间分布</h2>
                <p>色块面积代表目录占用空间大小</p>
              </div>
              <button className="secondary-button" onClick={startScan}>重新扫描</button>
            </div>
            <Treemap {...chartConfig} data={data} />
          </div>
        </section>
      )}
    </main>
  );
}

export default App;
