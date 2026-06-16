# 远程主机健康与稳定性 (remote-ssh-stability)

> 本目录归档远程主机连接健康全生命周期的演进：P0 自愈闭环 + P1 可见性增强（最高优先）、用户可控主动探活 + InfoBar 抖动抑制，以及可选 UI 收尾。
> 目标：远端主机断连后能“主动发现 + 状态明显可见 + InfoBar 不噪音”，同时允许用户完全关闭后台探测，避免对老用户产生意外流量。

---

## 本专题范围

- **P0**：后端自愈闭环（ServerManager event_sink + get_live_host / mark_unhealthy + ConnectionHealth + HostConnectionLost/Recovered 事件 + Tauri wiring）
- **P1 可见性**（用户最高优先）：HostSwitcher 强提示（red dot / border / “连接中断”）、ServerCard 健康详情（连续失败次数 + 最近失败原因 + 三层颜色区分）
- **P1 主动探活**（用户第二优先，按确认决策）：AppSettings 开关（默认低频开启 30s）+ 间隔配置 + ServerManager 后台 walker（仅 connected 主机、廉价 is_healthy、MissedTickBehavior::Skip）+ Tauri 条件 spawn/restart（启动期 + set_app_settings 热响应）
- **P1 InfoBar 抑制**（用户第三优先）：useHostHealthAlerts 增加 consecutiveFailures 阈值（>=2 才推 danger InfoBar；cf=1 短暂抖动只改状态/视觉）
- **可选收尾**：探活间隔改用滑块（RuntimeTab）、BotCard/IdentityTab 小视觉强化（unreachable chip 置前、notice 改 danger）
- **不含**：Docker 场景（见 docker 相关专题）、本机 Windows 直连（本地 host 不触发 host 事件）

真机点验：P0 全场景 + P1 主动探活 + InfoBar 抑制闭环已通过。

---

## 文档索引

### 1. 核心计划文档

| 文件 | 说明 | 状态 |
|------|------|------|
| [remote-ssh-stability.md](./remote-ssh-stability.md) | **完整计划 + 大白话需求 + 任务清单 + 偏差记录 + 完成标准**（P0-1~P0-12 + P1 全批次） | ✅ 已完成并归档 |

**阅读顺序建议**：
1. 先读 `remote-ssh-stability.md` §1~§5（理解痛点、P0 自愈原语、P1 优先级与用户确认决策）
2. 再读 §6~§7（实现细节 + 偏差 + 真机验证记录）
3. 最后看代码位置与提交历史

### 2. 关键设计决策速查

- **AppSettings 扩展**：`remote_host_health_probe_enabled`（默认 true，低频 ON）+ `remote_host_health_probe_interval_ms`（clamp 10s~5min）+ normalize → 见 plan §5 + domain/app_config.rs
- **后台 walker 生命周期**：条件 spawn/restart（set_app_settings 检测 enabled 变化先 cancel 旧再按新 enabled 启动）；walker 内部每 tick 读 Arc<RwLock<AppSettings>>，不直接推 InfoBar → 见 server_manager.rs + lib.rs
- **抖动抑制阈值**：consecutive_failures >= 2 才推 danger InfoBar（cf=1 只改视觉状态）；与 walker 递增计数 + useHostHealthAlerts key 抑制 + globalInfoBarStore 协同 → 见 useHostHealthAlerts.ts
- **可见性三层**：state（粗）+ health.consecutiveFailures + last_failure_reason（细）；HostSwitcher 红点/边框，ServerCard meta 详情行 danger/warning → 见 P0-12 实现
- **前端数据打通**：ServerProfile.health 透传 + useIsHostReachable + useHostConnectionEvents（invalidate + 恢复时 refetchQueries）+ useHostHealthAlerts（边沿检测）

---

## 相关代码位置

### 前端 Domain / 辅助（纯函数 + clamp）
```
src-ui/core/domain/remote-host/healthProbeSettings.ts   # clamp + 常量（对齐 Rust）
src-ui/core/domain/performance/performanceSettings.ts   # 参考实现
src-ui/core/services/settings.service.ts                # BackendSettings + fromDto/toDtoInvoke
src-ui/modules/settings/settings-draft.ts                 # draft + backendSlice + isSettingsDirty
src-ui/modules/settings/_shared.tsx                     # RemoteHostHealthProbeIntervalSlider（P1 收尾）
```

### 前端 Hooks（常驻 + 事件驱动）
```
src-ui/hooks/remote/
├── useHostHealthAlerts.ts          # 核心：consecutiveFailures 阈值门禁 + 抑制 + 事件双路径
├── useHostConnectionEvents.ts      # invalidate + recovered 时 refetchQueries（恢复链路增强）
└── useIsHostReachable.ts           # 门禁与视觉共用

src-ui/hooks/ui/globalInfoBarStore.ts  # key 顶替 + onUserDismiss 抑制（被复用）
```

### 前端 UI 集成点
```
src-ui/modules/settings/tabs/RuntimeTab.tsx          # “远程主机健康监控”分区（开关 + 滑块，P1 收尾）
src-ui/modules/bot/list/next/BotCard.tsx             # 主机不可达 chip（置前强化）+ danger accent + 启动阻断
src-ui/modules/bot/config/next/IdentityTab.tsx       # 远端不可达 danger notice（视觉强化）
src-ui/modules/remote/...                            # HostSwitcher / ServerCard（P1 可见性第一批）
```

### Tauri / 命令层（薄壳 + 热更新）
```
src-tauri/src/lib.rs
src-tauri/src/commands/app_settings.rs   # set_app_settings 写回 + normalize + 条件 spawn/cancel walker
```

### 后端核心（Layer 1-3）
```
crates/ncd-domain/src/app_config.rs          # AppSettings 字段 + default_true + clamp + normalize_*
crates/ncd-runtime/src/server_manager.rs     # run_health_probe_loop（P1 主动探活核心） + mark_unhealthy_internal + publish lost
crates/ncd-runtime/src/events.rs             # HostConnectionLost { server_id, reason, consecutive_failures } + Recovered
crates/ncd-host/src/host.rs                  # is_healthy trait 方法（RemoteLinuxHost 廉价 ping 实现）
```

---

## 演进历史（按时间倒序）

| 日期       | 事件 | 文档/提交 |
|------------|------|-----------|
| 2026-06-17 | 完成可选收尾：探活间隔改滑块 + BotCard/IdentityTab 小视觉强化（unreachable chip 置前、notice 改 danger） | commit `354fae3a` |
| 2026-06-17 | P1 InfoBar 抖动抑制完成（consecutiveFailures >= 2 才推 danger；cf=1 只改状态）+ 真机点验 | commit `e3ccbadc`；计划 §5 抑制项标记完成 |
| 2026-06-17 | P1 主动探活核心闭环完成（AppSettings 默认低频 ON + walker + 条件 spawn/restart wiring） | commits `fee97416`、`7891a8bc`、`ec8ddb3c` |
| 2026-06-17 | 用户确认决策：默认低频开启、本批含间隔、字段 remote_host_health_probe_enabled、walker 条件 spawn/restart | 计划 §7 记录 |
| 2026-06-16~17 | P1 可见性第一批（HostSwitcher 红点/边框 + ServerCard 健康详情行 + 三层区分） | commits `82f63267`、`87e8eb74` |
| 2026-06-16 | P0-12 UI 三层失败区分 + InfoBar key 抑制 + 恢复链路增强（refetchQueries） | commit `060b4227` + `08de10e0` |
| 2026-06-15~16 | P0-10~P0-11 后端自愈闭环 + 前端 hooks（ServerManager event_sink + get_live_host/mark_unhealthy + useHostConnectionEvents） | commit `5cc67a63` 等 |
| 2026-06-17 | 真机 M1-M8 场景点验通过（P0 全绿 + P1 主动探活 + InfoBar 抑制闭环） | 用户确认 |

---

## 未来待办（来自原计划 §P2 及收尾建议）

- [ ] 更精细的驱逐策略（连续 N 次 + 时间窗口才真正驱逐，减少瞬断误杀）
- [ ] 日志跟随类（RemoteBotLogFollow）连续失败 N 次后主动请求 source.refresh
- [ ] 可选 `BotTransportStateChanged` 事件 + 更细粒度前端消费
- [ ] 设置页进一步暴露 probe interval 预设（低/中/高频）或历史健康图表
- [ ] 架构 lint / 更多真机回归用例加入 CI

---

## 参考资料

### 旧版实现（迁移参考）
- `.references/legacy-python/` 中与远程主机健康/SSH 相关的占用卡片、连接测试逻辑（P1 可见性与 walker 设计时曾参考 occupancy_card.py）

### 相关文档
- [docs/dev/architecture/README.md](../README.md)（架构文档总索引）
- [docs/context/codemap.md](../../../context/codemap.md)（功能域 → 代码落点）
- [docs/context/lessons.md](../../../context/lessons.md)（历史踩坑）
- [docs/context/frontend.md](../../../context/frontend.md)（前端分层与 hooks 铁律）
- 本专题归档前计划原位置（已移动）：`docs/dev/architecture/remote-ssh-stability.md`（现为本目录副本）

---

**专题维护者**: Claude (Opus 4.8) + 用户真机验证  
**最后更新**: 2026-06-17  
**状态**: 已完成归档（P1 核心 + 可选收尾 + 真机点验全部通过）
