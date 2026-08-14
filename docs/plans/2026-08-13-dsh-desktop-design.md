# DSH Desktop — 应用壳子设计

日期：2026-08-13

## 目标

用 Rust 开发一个 Windows 桌面应用壳子，替代 `npm install -g @deepseek-ai/dsh` 的手动安装/升级流程：

- 打开后托管 DSH 后台进程（`dsh web`，Web 界面服务）。
- 用内嵌 WebView2 窗口显示 DSH 界面。
- 应用关闭（含崩溃）后清理后台子进程。
- 自带初始化/安装界面：启动时检测环境缺失或需要升级就显示。

## 关键事实（来自对 dsh CLI 的实测）

- `dsh web` 是 `dsh --profile web` 的别名，启动一个 node:http 服务。
- 默认监听 `127.0.0.1:3080`；支持 `--port 0` 让系统分配空闲端口。
- 启动后在 stdout 打印 `dsh web: http://127.0.0.1:<port>`（实际端口以此为准）。
- bin 入口：`<install>/node_modules/@deepseek-ai/dsh/lib/bin.js`，用 node 运行。
- CLI 是 Node.js ESM 程序，运行依赖系统 Node。

## 技术选型

- 窗口 + WebView2：`tao 0.36`（窗口/事件循环）+ `wry 0.56`（WebView2 封装）。
- 进程清理：`windows 0.61` 的 Job Object（`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`）。
- 安装/升级：直接调用系统 npm（Node 自带 TLS，稳定）。

## 架构

单一 WebView2 窗口，双模式：

1. **初始化界面**（本地 HTML，`with_html` 载入）：检测 Node、安装/升级进度、
   错误信息、"发现新版本，是否升级"按钮。状态由 Rust 经 `evaluate_script`
   推送给页面；按钮经 wry IPC（`window.ipc.postMessage`）回传 Rust。
2. **主界面**：DSH 就绪后 `load_url` 跳转到 `http://127.0.0.1:<port>`。

## 组件

- `paths.rs`：应用数据目录 `%LOCALAPPDATA%\DSHDesktop\`（`runtime/`、`logs/`）。
- `node.rs`：从 PATH 探测 node.exe 与 npm-cli.js。
- `runtime.rs`：读已装版本、`npm view` 查最新、`npm install --prefix` 安装/升级。
- `process.rs`：Job Object 创建；spawn `node bin.js web`；解析 stdout 端口；
  关闭时终止整个进程树。
- `init_ui.rs`：内置初始化页 HTML/JS 常量。
- `main.rs`：事件循环、IPC、生命周期线程、导航。

## 生命周期流程

1. 启动：建 Job Object → 建窗口 + webview → 载入初始化页。
2. 后台线程：探测 Node → 检查安装 → 缺失则安装 → spawn `dsh web` →
   解析端口 → 发 `ready` → 主线程导航；并行查最新版本，有新版弹升级提示。
3. 升级动作：停子进程 → npm 装最新 → 重新 spawn → 重新导航。
4. 关闭：`TerminateJobObject` 杀整树 → 退出（KILL_ON_JOB_CLOSE 兜底崩溃场景）。

## 错误处理

- Node 缺失 / 安装失败 / 端口冲突：错误文本写入初始化页 + `logs/` 日志文件。
- AssignProcessToJobObject 失败（进程已在别的 job 里）时回退到
  `child.kill()` + `taskkill /T /F`。

## 构建

- 正常环境：`cargo build --release`。
- 本沙箱（schannel TLS 被禁）：用 `tools/cargo-mirror.mjs` 做本地 HTTP 镜像，
  `cargo build --config ...` 指向镜像。
