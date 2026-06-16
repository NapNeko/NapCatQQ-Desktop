# remote-ssh-stability 归档创建摘要

**创建时间**: 2026-06-17  
**归档原因**: P0 自愈闭环 + P1 可见性（最高优先）+ 主动探活（用户可开关）+ InfoBar 抖动抑制 + 可选 UI 收尾全部完成并经真机点验通过。原计划文档 `docs/dev/architecture/remote-ssh-stability.md` 已移动至本目录，作为专题主文档。

**归档动作**:
- mkdir -p docs/dev/archive/bugfix/remote-ssh-stability
- mv docs/dev/architecture/remote-ssh-stability.md docs/dev/archive/bugfix/remote-ssh-stability/remote-ssh-stability.md
- 新建本目录 README.md（严格对齐 remote-direct-run 风格：范围、索引、决策、代码位置、历史、待办、参考）
- 提交时使用 `git add -f`（因 docs/dev 可能受 .gitignore 影响）

**包含内容**:
- README.md（本专题完整索引）
- remote-ssh-stability.md（原完整计划 + 所有 §1~§8 内容 + 偏差 + 完成标准，已在归档前持续更新）

**相关提交（P1 批次）**:
- 354fae3a (可选收尾)
- e3ccbadc (InfoBar 抑制)
- ec8ddb3c (walker + wiring)
- 7891a8bc (前端 draft+UI)
- fee97416 (Domain 字段)
- 更早 P0-12 / P0-11 / P0-10 可见性与自愈提交

**后续维护**:
- 本目录为只读归档。
- 如有新演进，在新专题或主架构文档中记录，并在此 README 的“演进历史”追加条目（或新建子文档）。
- 真机验证记录已融入原计划 §7 及本 README 历史表。

**创建者**: Claude Code (按用户“收尾完成，真机点验完成，请把计划像 remote-direct-run 一样归档”指令执行)
