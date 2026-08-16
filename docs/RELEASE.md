# DSH Desktop 发布流程

本文档记录从代码修改到正式 Release 的完整流程。每次发布照此执行。

## 版本号约定

- 语义化版本：`MAJOR.MINOR.PATCH`
- 版本号只写在一处：`Cargo.toml` 的 `version` 字段
- CI 打包时自动从 `Cargo.toml` 读取版本号写入 Info.plist / 安装包文件名，**不要手改 workflow 里的版本**

## 发布步骤

1. **确认改动已全部提交并推送**

   ```bash
   git add -A
   git commit -m "<改动说明>"
   git push origin main
   ```

2. **更新版本号**（三处同步：Cargo.toml、如已生成则 Cargo.lock、git tag）

   ```bash
   # 改 Cargo.toml 里的 version = "x.y.z"
   cargo check          # 刷新 Cargo.lock 里的版本
   git add -A && git commit -m "v<x.y.z>"
   git push origin main
   ```

3. **打 tag 触发 Release 构建**

   ```bash
   git tag v<x.y.z>
   git push origin v<x.y.z>
   ```

3.5 **写 Release 说明文件**（发布前，不是发布后！）：

   ```bash
   # 新建 docs/release-notes/v<x.y.z>.md，首行固定为：
   #   # v<x.y.z> Release Notes（存档）
   # 其余内容按下面"Release Notes 模板"写
   git add docs/release-notes/v<x.y.z>.md
   ```

   这个文件是 Release 说明的唯一来源：打 tag 后 CI 自动把它的正文
   （去掉首行标题）发布为 GitHub Release body。没有这个文件时，CI 回退到
   GitHub 自动生成的 release notes。文件同时留在仓库里做历史存档。

4. **等待 CI**（约 10–15 分钟，三平台并行）：  
   https://github.com/fanfanv5/dsh-desktop/actions

5. **验证 Release**：确认 Release 说明来自 `docs/release-notes/v<x.y.z>.md`、三个平台的资产都在。需要时本地自测安装包：

   ```bash
   # macOS
   curl -LO https://github.com/fanfanv5/dsh-desktop/releases/download/v<x.y.z>/DSH-Desktop-macos-universal.dmg
   # 打开 DMG → 双击 "Install DSH Desktop.command"
   ```

## Release Notes 模板

每次发布按此结构写（中英双语可选，开源项目建议英文为主、附中文）：

```markdown
## Highlights

<一句话总结这个版本最重要的 1–3 个变化>

## Changes

### Added
- <新增功能，引用 PR/issue 编号如果有>

### Fixed
- <修复的 bug：症状 → 根因 → 修法，一段一个>

### Changed
- <行为变化、重构、依赖升级>

## Install

| Platform | Asset |
|---|---|
| macOS (universal) | DSH-Desktop-macos-universal.dmg |
| Windows x64 | DSH-Desktop-Setup-x.y.z.exe |
| Linux x64 | DSH-Desktop-linux-x64.tar.gz |

### macOS
1. Download the DMG, open it.
2. Double-click **Install DSH Desktop.command** (installs to /Applications, removes quarantine).
3. First launch asks **once** for removable-volume / disk access — click Allow.
   With the stable signing identity, all future updates keep that permission silently.

### Windows
Run the installer (Inno Setup).

### Linux
```bash
tar xzf DSH-Desktop-linux-x64.tar.gz && cd DSHDesktop && sudo ./install.sh
```
```

## 签名说明（重要）

- **macOS 本地构建**：`tools/make-app.sh` 使用本地自签名证书（"DSH Local Codesign"，100 年有效期），
  签名身份固定后 TCC 授权（完全磁盘访问/可移动卷）跨版本延续，不再反复弹窗。
- **CI 构建**：未配置 `MACOS_SIGNING_P12` / `MACOS_SIGNING_PASSWORD` secrets 时自动降级 ad-hoc 签名
  （构建日志有警告）。装 ad-hoc 版本会在首次启动时弹一次权限窗，属预期。
  想让 CI 版也免弹窗：`tools/export-signing-identity.sh` 导出证书 → 填入仓库 secrets。
- 证书创建：`tools/make-signing-identity.sh`（一次性，无交互）。
- 详细背景见 docs/plans 或提交历史中 "stable signing" 相关提交。

## 常见问题

- **tag 打错了想重发**：`git tag -d v<x.y.z> && git push origin :refs/tags/v<x.y.z>`，改完重打重推。
- **CI 失败**：看 Actions 日志；macOS job 失败多半是签名脚本问题（无 secrets 时应走降级路径而非报错）。
- **本地测试新打包**：`tools/make-app.sh`（含签名）→ `tools/make-dmg.sh`（打 DMG）。
