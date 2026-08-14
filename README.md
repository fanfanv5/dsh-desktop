# DSH Desktop

一个用 Rust 开发的 Windows 桌面应用壳，替代 "npm install -g @deepseek-ai/dsh" 的手动安装/升级流程。

- 打开后自动托管 DSH 后台进程（dsh web）。
- 用内嵌 WebView2 窗口显示 DSH 界面。
- 应用关闭（含崩溃）后清理整个后台子进程树。
- 自带初始化/安装界面：启动时检测环境缺失或需要升级就显示。

## 功能

1. 环境检测：从 PATH 探测 Node.js 与 npm；缺失时在初始化界面给出提示和"重试"。
2. 安装/升级：把 @deepseek-ai/dsh 装到 %LOCALAPPDATA%/DSHDesktop/runtime，启动时后台检查最新版，有新版本弹"立即升级 / 跳过"。
3. 托管后台进程：spawn node <runtime>/.../bin.js web，解析其 stdout 打印的
   "dsh web: http://127.0.0.1:<port>" 得到真实地址，再让 WebView 跳转过去。
4. 进程清理：用 Windows Job Object（JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE）托管子进程，
   关闭窗口时 TerminateJobObject 杀掉整个进程树；即使应用崩溃，句柄关闭也会连带清理。
5. 初始化界面：未就绪时 WebView 显示内置 HTML 页（状态、进度、错误、升级按钮），
   就绪后跳转到 DSH 界面。按钮通过 wry IPC 回调到 Rust。

## 依赖

- Windows 10/11（内置 WebView2 / Edge 运行时）。
- 系统已安装 Node.js（建议 LTS），且 node 在 PATH 中。
- Rust 工具链（仅构建时需要）：rustup + MSVC target。

## 构建

~~~powershell
cargo build --release
~~~

产物：target/release/dsh-desktop.exe

> 注意：在普通（非沙箱）环境直接执行 cargo build --release 即可，cargo 会正常走 crates.io。

## 运行

双击 dsh-desktop.exe。首次运行会自动安装 @deepseek-ai/dsh（需要联网）。

数据目录（%LOCALAPPDATA%/DSHDesktop）：

- runtime/ —— 托管安装的 @deepseek-ai/dsh。
- logs/dsh-web.out.log 与 logs/dsh-web.err.log —— 后台进程日志。

## 目录结构

~~~
src/
  main.rs      入口：窗口 + WebView2 + 事件循环 + IPC + 清理
  lifecycle.rs 后台控制器：检测/安装/升级/spawn 状态机
  process.rs   进程管理：Job Object + spawn dsh + URL 解析
  runtime.rs   npm 安装/升级/版本查询
  node.rs      Node/npm 探测
  paths.rs     数据目录解析
  events.rs    线程间事件/命令类型
  init_ui.rs   初始化界面 HTML
tools/
  cargo-mirror.mjs  本地 crates.io HTTP 镜像（仅沙箱构建用）
docs/plans/        设计文档
~~~

## 沙箱构建（仅在本开发环境需要）

本会话的沙箱禁用了 cargo 的 schannel TLS（SEC_E_NO_CREDENTIALS），所以用
tools/cargo-mirror.mjs 起一个本地 HTTP 镜像，让 cargo 走纯 HTTP：

终端 A，启动镜像：

~~~powershell
node tools/cargo-mirror.mjs 8899
~~~

终端 B，构建（把 crates-io 指向镜像）：

~~~powershell
$env:CARGO_HOME = "$PWD/.cargo-home"
$env:CARGO_TARGET_DIR = "$PWD/target"
cargo build --release --config 'source.crates-io.replace-with="mirror"' --config 'source.mirror.registry="sparse+http://127.0.0.1:8899/index/"'
~~~

普通环境不需要镜像，直接执行 cargo build --release。
