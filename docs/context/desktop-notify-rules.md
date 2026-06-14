# 桌面通知（Toast）规则

> 与 `lightweight-mode-design.md` P0 通知相关；避免 Bot 配置与 App 设置重复开关。

## 三类事件

| 事件 | 谁控制 | 后端路径 |
|------|--------|----------|
| NapCat **登录态离线**（曾在线 → 离线） | **Bot 配置** → 高级 →「掉线时下发桌面通知」`advanced.offline_notice` | `NapCatLoginPoller` → `OfflineNotifier` |
| **进程异常退出** | **App 设置** → 外观 → 窗口 →「Bot 异常退出时通知」`notifyOnBotCrashed` | `DomainEvent::BotProcessExited` 监听 |
| **踢线 / 登录失效** | **App 设置** →「被踢下线时通知」`notifyOnLoginKicked` | `DomainEvent::NapCatLoginInvalidated` 监听 |

## 已废弃 / 勿再叠一层

- **AppSettings.notifyOnOffline**：已从设置页移除；磁盘字段可保留默认 `true` 做 round-trip，**不参与** Poller 与 Toast。
- **WebUiPollerSettings** 的 `botOfflineWebHookNotice` / `botOfflineEmailNotice`：设置页未暴露，后端仍为 noop/预留，**不**与桌面 Toast 混用。

## Windows Toast 文案

- WinRT：`title` = 事件标题（如「Bot 离线」），`text1` = 详情一行。
- Toast **来源行**（应用名）由安装包 / AUMID（`identifier`）决定，与 `title` 不是同一字段；MSI 安装后应显示产品名而非 PowerShell。

## 实现落点

- `src-tauri/src/windows_toast.rs` — WinRT + AUMID
- `src-tauri/src/desktop_notify.rs` — `TauriOfflineNotifier` + 事件监听
- `crates/ncd-runtime/src/bot_manager.rs` — `PollerConfig.offline_notice_enabled` ← `bot_cfg.advanced.offline_notice` 仅此条件