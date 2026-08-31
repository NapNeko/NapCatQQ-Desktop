# SnowLuma 原生扫码二维码刷新设计

## 目标

为远端 Linux Native SnowLuma 的一次性二维码提取结果增加“重新提取”能力。刷新只重新调用现有 Desktop QR command，在同一个弹窗内替换结果；不引入事件流或新的运行时生命周期。

## 当前后端契约

UI 使用现有服务方法：

`botService.startSnowlumaQrLogin(botId): Promise<SnowlumaQrLoginResult>`

`SnowlumaQrLoginResult` 是由 Rust `ts-rs` 生成并在 `src-ui/core/ipc/types.ts` re-export 的判别联合：

- `status: "payload"`：包含 `session` 与内存中的 `payload` 字符串。
- `status: "fallback_no_vnc"`：包含 `session` 与受限的 `reason` 分类。

该 command 是一次性调用。每次刷新都会获得一次新的 command 结果和 session，不假设旧 session 可续期，也不向后端发送刷新 token。浏览器预览继续使用 service 的 inert fallback。

## 交互与数据流

1. 远端 Native SnowLuma Bot 活跃且 `snowlumaLoginState !== "logged_in"` 时，BotCard 显示独立于 NapCat 的扫码按钮。
2. 首次点击后，BotCard 将该卡片标记为 pending，打开 SnowLuma 专用弹窗，并调用 `botService.startSnowlumaQrLogin(botId)`。
3. 弹窗在等待结果时显示加载状态；结果返回后在同一弹窗内渲染 Payload 或 fallback 分支。
4. Payload 分支使用现有 `qrcode` 包在本机生成 SVG。Payload 只作为 React 内存状态传入渲染组件，不展示原文。
5. 用户点击“重新提取”时，保持当前弹窗打开，并把当前 result 作为仅限本次请求的 volatile rollback 值；显示状态立即切换为 null/loading，重新进入 pending，然后再次调用同一个 service 方法。成功后用新结果替换 rollback 值。
6. 刷新请求完成或失败后清除 pending。成功时丢弃 rollback 值；失败时恢复请求前 result（若请求前无 result，则显示可重试的错误状态）。pending 期间按钮和刷新入口均 disabled，重复点击不得产生第二个并发 command。
7. 关闭弹窗时清除本地 result 和任何 rollback 值；重新打开从空结果开始，不复用上一次 Payload。

## BotCard 与 BotListPage 边界

- `BotCard` 继续保留现有 NapCat `qrcodeUrl`、`QrCodeDialog` 和其登录成功/踢线行为，不共用 SnowLuma result 或状态。
- `BotCard` 只负责入口可见性、按卡片隔离 pending/result、打开弹窗以及将刷新/关闭动作绑定到自身状态。
- `BotListPage`/`BotListGrid` 负责把 `botService.startSnowlumaQrLogin` 作为已存在的 typed callback 传入，不在页面中复制后端结果形状。
- SnowLuma noVNC 仍使用现有 `onOpenNovnc`/`useOpenSnowlumaNovnc` 路径，刷新不创建第二套 noVNC command，也不改变隧道、端口、密码或剪贴板语义。

## 弹窗结果与 fallback

`SnowlumaQrCodeDialog` 接受 `result: SnowlumaQrLoginResult | null`、`open`、`onOpenChange`、Bot ID、刷新回调和现有 noVNC 回调。

- `null`：显示提取进行中的 loading 状态。
- `payload`：显示可扫描 QR；二维码输入只进入本地 qrcode 编码器，禁止网络请求。
- `fallback_no_vnc`：显示固定的用户可读说明和 `reason` 到文案的白名单映射，例如能力不可用、窗口绑定不唯一、截图失败或解码失败。不得把原始后端错误字符串直接作为常驻 UI 文案。
- fallback 分支提供“打开 noVNC 桌面”按钮；仅在现有 callback 存在时渲染，调用该 callback，不伪造成功或 Payload。用户仍可关闭弹窗或重新提取。

## 错误路径

- 首次 command reject：停止 pending、关闭本次弹窗、通过现有 InfoBar 显示“SnowLuma 二维码提取失败”和安全的错误文本；不写日志、不写入 Payload。
- 刷新 command reject：停止 pending、保留弹窗打开并恢复刷新前已经显示的 result；若刷新前没有 result，则显示固定的可重试错误状态。通过 InfoBar 提示失败；不得自动打开 noVNC 或重启 Bot。
- Payload 渲染失败：弹窗显示固定的二维码渲染失败文案，仍允许重新提取和关闭；不展示 Payload 原文。
- fallback/noVNC callback reject：沿用现有 BotListPage noVNC 错误 InfoBar，不改变 QR result，也不重复调用 command。
- Bot 停止、切换或组件卸载：本地 result 和 rollback 值随卡片卸载/关闭释放；不增加后台轮询或取消协议。

## 敏感数据处理

- Payload 仅存在于一次 command 响应、BotCard state 和 QR 渲染组件内存中。
- 不持久化、不写日志、不复制剪贴板、不发送第三方、不把 Payload 放进 URL、InfoBar、错误文本或测试快照。
- `reason` 只通过固定白名单映射展示。测试使用虚构 Payload，不能提交真实二维码截图或凭据。

## 测试设计

遵循现有 Vitest + Testing Library 约定：

1. `src-ui/core/domain/bot/snowluma-remote-ui.test.ts` 覆盖可见性谓词：仅远端 Native、active、非 `logged_in` 返回 true；Docker、本机、非 active、已登录返回 false。
2. `SnowlumaQrCodeDialog.test.tsx` 覆盖 Payload 分支最终出现本地 QR 容器且不渲染 Payload 原文。
3. 同一测试文件覆盖 fallback 分支显示白名单文案并调用既有 noVNC callback。
4. 增加 refresh 行为测试：点击刷新触发第二次 service callback，弹窗不卸载；promise 未完成时再次点击不会增加调用次数；第二次 Payload 替换第一次结果。
5. 增加 command reject 测试：pending 结束、错误可见且不会泄漏 Payload；刷新失败恢复既有 result；没有既有 result 时显示固定的可重试错误状态。
6. 测试应隔离现有 GSAP motion 依赖，避免动画插件初始化影响测试收集；不运行或引入后端测试替身。

## 明确不做

- 不新增 DomainEvent QR status/payload 事件、event-stream 名称、aggregator、store 或 `useSyncExternalStore` hook。
- 不新增 cancel command、refresh command 或后端 session registry；“刷新”只是 UI 再调用现有一次性 start 方法。
- 不实现 QR 过期监听、在线状态状态机、窗口重建、SSH 重连、后台轮询或完整登录生命周期。
- 不修改 `ncd-domain`、`ncd-runtime`、`src-tauri`、SnowLuma backend、远端资产、QQ、noVNC 隧道或持久化 Bot 配置。
- 不改变现有 NapCat QR 流程、NapCat QR dialog、SnowLuma noVNC callback 的行为。
