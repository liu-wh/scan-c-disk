# 磁盘管家

一个基于 Tauri、React 和 Rust 的 Windows 磁盘空间分析工具。应用会扫描 C 盘目录，统计目录占用空间，并通过矩形树图直观展示空间分布，支持点击色块逐级下钻。

## 功能特性

- 扫描 C 盘目录并计算文件占用空间。
- 使用矩形树图展示目录大小，色块面积代表占用空间。
- 支持点击色块进入子目录并查看下一级内容。
- 显示扫描位置、总占用空间和目录数量。
- 自动处理重复目录名，避免图表节点冲突。
- 扫描时排除以下系统目录：
  - `C:\windows\winsxs`
  - `C:\programdata\microsoft\windows\wer`

## 技术栈

- **桌面应用**：Tauri 2
- **前端**：React 19、Vite
- **后端**：Rust、Tokio
- **图表**：Ant Design Plots

## 环境要求

- Windows
- Node.js 18 或更高版本
- pnpm
- Rust 工具链
- Tauri 2 的系统依赖

可参考 [Tauri 官方 Windows 配置指南](https://tauri.app/start/prerequisites/#windows) 安装系统依赖。

## 安装依赖

```bash
pnpm install
```

## 开发运行

启动前端开发服务器：

```bash
pnpm dev
```

启动 Tauri 桌面应用：

```bash
pnpm tauri dev
```

## 构建

构建前端：

```bash
pnpm build
```

构建可发布的 Tauri 应用：

```bash
pnpm tauri build
```

构建产物位于 `src-tauri/target/release/` 目录。

## 项目结构

```text
src/
  App.jsx          应用界面、扫描状态和矩形树图
  App.css          页面样式
  main.jsx         React 入口
src-tauri/
  src/command.rs   Rust 扫描命令和目录树构建逻辑
  src/lib.rs       Tauri 命令注册
  tauri.conf.json  Tauri 应用配置
```

## 扫描说明

扫描命令默认接收以下参数：

```json
{
  "root": "C:\\",
  "max_depth": 5
}
```

扫描会跳过符号链接和指定的系统目录。部分目录可能因权限不足无法读取，这些目录会按当前可读取内容参与统计。
