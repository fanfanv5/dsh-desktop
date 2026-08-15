# DSH Desktop

一个用 Rust 开发的 Windows / macOS 桌面应用壳，替代 "npm install -g @deepseek-ai/dsh" 的手动安装/升级流程。

- 打开后自动托管 DSH 后台进程（dsh web）。
- 用内嵌 WebView 窗口（Windows: WebView2 / macOS: WKWebView）显示 DSH 界面。
- 应用关闭（含崩溃）后清理整个后台子进程树。
- 自带初始化/安装界面：启动时检测环境缺失或需要升级就显示。

## 功能

1. 环境检测：从 PATH 探测 Node.js 与 npm（macOS 上从 Finder/.app 启动时 PATH 不含 Homebrew 等目录，会额外扫描 /opt/homebrew/bin、/usr/local/bin、~/.volta/bin、~/.nvm/versions/node/*/bin）；缺失时在初始化界面给出提示和"重试"。
2. 安装/升级：把 @deepseek-ai/dsh 装到 %LOCALAPPDATA%/DSHDesktop/runtime（macOS: ~/Library/Application Support/DSHDesktop/runtime），启动时后台检查最新版，有新版本弹"立即升级 / 跳过"。
3. 托管后台进程：spawn node <runtime>/.../bin.js web，解析其 stdout 打印的
   "dsh web: http://127.0.0.1:<port>" 得到真实地址，再让 WebView 跳转过去。
4. 进程清理：Windows 用 Job Object（JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE）托管子进程，
   关闭窗口时 TerminateJobObject 杀掉整个进程树；即使应用崩溃，句柄关闭也会连带清理。
   macOS/Linux 用独立进程组（process_group + kill(-pgid)）达到同样效果。
5. 初始化界面：未就绪时 WebView 显示内置 HTML 页（状态、进度、错误、升级按钮），
   就绪后跳转到 DSH 界面。按钮通过 wry IPC 回调到 Rust。
6. 单实例：Windows 用命名互斥体，macOS/Linux 用数据目录下的 flock 锁文件；
   两个实例并发跑 npm 安装会互相破坏 runtime 状态。
7. 窗口定制：Windows 去掉原生标题栏（NC 子类化，保留原生缩放/贴靠），
   页内注入 min/max/close 按钮栏；macOS 用透明标题栏 + 全尺寸内容视图，
   保留原生红绿灯按钮，页内注入纯拖拽条（双击缩放）。两平台的标题栏
   颜色都跟随 dsh 明暗主题。

## 依赖

- Windows 10/11（内置 WebView2 / Edge 运行时），或 macOS 11+（系统 WKWebView）。
- 系统已安装 Node.js（建议 LTS）。命令行启动要求在 PATH 中；
  macOS 从 Finder/.app 启动时额外扫描 Homebrew / Volta / nvm 的常见安装位置。
- Rust 工具链（仅构建时需要）：rustup + MSVC target（Windows）或 stable host 工具链（macOS）。

## 构建

~~~powershell
cargo build --release
~~~

产物：target/release/dsh-desktop.exe（Windows）/ target/release/dsh-desktop（macOS）

> 注意：在普通（非沙箱）环境直接执行 cargo build --release 即可，cargo 会正常走 crates.io。

## 运行

- Windows：双击 dsh-desktop.exe。
- macOS：推荐用 .app 包（Release 流水线会打出 universal 的 DSH Desktop.app /
  dmg，带图标）；直接运行裸二进制也可以，Dock 图标会是默认的。本地产出 .app：

  ~~~sh
  mkdir -p "DSH Desktop.app/Contents/MacOS" "DSH Desktop.app/Contents/Resources"
  cp target/release/dsh-desktop "DSH Desktop.app/Contents/MacOS/"
  cp assets/dsh-desktop.icns "DSH Desktop.app/Contents/Resources/"
  # Info.plist 参考 .github/workflows/release.yml 的 macOS 打包步骤
  ~~~

首次运行会自动安装 @deepseek-ai/dsh（需要联网）。

数据目录：

- Windows：%LOCALAPPDATA%/DSHDesktop
- macOS：~/Library/Application Support/DSHDesktop

目录内容：

- runtime/ —— 托管安装的 @deepseek-ai/dsh。
- logs/dsh-web.out.log 与 logs/dsh-web.err.log —— 后台进程日志。

## 目录结构

~~~
src/
  main.rs      入口：窗口 + WebView + 事件循环 + IPC + 清理 + 页内标题栏注入
  lifecycle.rs 后台控制器：检测/安装/升级/spawn 状态机
  process.rs   进程管理：Job Object（Win）/ 进程组（Unix）+ spawn dsh + URL 解析
  borderless.rs Windows 无边框窗口（NC 子类化）
  runtime.rs   npm 安装/升级/版本查询
  node.rs      Node/npm 探测（macOS 含 Finder PATH 兜底扫描）
  paths.rs     数据目录解析
  events.rs    线程间事件/命令类型
  init_ui.rs   初始化界面 HTML
tools/
  make-icon.mjs     图标生成（.ico / icon-64.rgba / .icns）
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
