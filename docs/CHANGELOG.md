# 🚀 NapCatQQ Desktop 更新日志（v2.2.6）

## Tips
- v2.0 起为破坏性更新，旧版无法直接更新，请手动下载新版安装包。
- 安装完成后，可在设置页面迁移或重新导入旧版本配置。

<!-- BEGIN AUTO RELEASE NOTES -->
## 🐛 修复功能
- 修复关闭/退出仅阻止本地 Bot，远端 Bot 不再阻止退出
- 修复 SnowLuma 停止后立即重启导致的 `Signal source has been deleted` 崩溃
- 修复 SnowLuma 停止过程中快速重复操作引发的竞态问题，新增“关闭中…”状态防护

## ✨ 新增功能
- 远端 Bot 状态透传 `elapsed_seconds` 字段
- 启动期间自动重新附加远端 Bot 状态，并补建 SnowLuma 隧道
- 远端 SnowLuma Bot 卡片新增“打开 VNC 工具”按钮

## 🔧 优化功能
- 优化 NapCatQQLoginState 日志文案，统一使用中性词“Bot WebUI”
<!-- END AUTO RELEASE NOTES -->

## ⚠️ 重要提醒
- 如果遇到问题，请通过 GitHub Issue 反馈。
