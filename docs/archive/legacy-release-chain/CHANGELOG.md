# 🚀 NapCatQQ Desktop 更新日志（v2.2.8）

## Tips
- v2.0 起为破坏性更新，旧版无法直接更新，请手动下载新版安装包。
- 安装完成后，可在设置页面迁移或重新导入旧版本配置。

<!-- BEGIN AUTO RELEASE NOTES -->
## 🐛 修复功能
- 修复打包或精简环境下 httpx 证书缺失导致的闪退问题，并重构证书异常防御补丁，对底层 ssl.create_default_context 进行更健壮的修补
- 修复状态轮询线程在 QObject 销毁后触发的异常崩溃
<!-- END AUTO RELEASE NOTES -->

## ⚠️ 重要提醒
- 如果遇到问题，请通过 GitHub Issue 反馈。
