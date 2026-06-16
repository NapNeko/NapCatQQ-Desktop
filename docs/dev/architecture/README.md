# 架构设计文档索引

本目录包含 NapCatQQ Desktop 重大功能的架构设计文档。

## 目录

### 正在进行的设计

- **[coupling-audit.md](./coupling-audit.md)** - 耦合度审计报告  
  代码架构耦合度分析与改进建议。
  - 状态：持续更新

---

## 归档文档

历史完成的架构设计文档已归档至 `docs/dev/archive/bugfix/` 目录。

### 已归档的远程主机健康与稳定性专题（2026-06-17）

> P0 后端自愈闭环 + P1 可见性增强（最高优先）、用户可控主动探活（默认低频开启）+ InfoBar 抖动抑制（consecutiveFailures 阈值），以及可选 UI 收尾。已全部完成并经真机点验通过。

- **remote-ssh-stability/**（专题目录）
  - [README.md](../archive/bugfix/remote-ssh-stability/README.md) - 专题索引（范围、决策、代码位置、完整演进历史）
  - [remote-ssh-stability.md](../archive/bugfix/remote-ssh-stability/remote-ssh-stability.md) - 完整计划文档（P0~P1 全批次 + 偏差记录 + 完成标准）
    - 状态：已完成归档（2026-06-17）
    - 目标：远端主机断连主动发现、状态明显可见、InfoBar 不噪音、用户可完全关闭后台探测
  - 相关 Commit（P1 批次）：`fee97416`、`7891a8bc`、`ec8ddb3c`、`e3ccbadc`、`354fae3a`、`07a3df75`（归档）
  - 真机点验：P0 全场景 + P1 主动探活 + InfoBar 抑制闭环已通过

### 已归档的远程直接运行专题（2026-06-16）

> 解决「远端直接运行」问题的完整解法闭环，包括系统依赖、SSH 密码管理、启动门禁三个层面。

- **remote-direct-run/**（专题目录）
  - [remote-direct-run-gate.md](../archive/bugfix/remote-direct-run/remote-direct-run-gate.md) - 远程直接运行启动门禁体系
    - 状态：已完成（2026-06-16）
    - 目标：本地/远程直接运行的依赖缺失检测、配置时实时提示、保存/启动双重阻断
  - [qq-dependency-management.md](../archive/bugfix/remote-direct-run/qq-dependency-management.md) - QQ 系统依赖自动管理
    - 状态：已完成（2026-06-15）
    - 目标：解决远端 Linux QQ 缺少系统依赖导致启动失败
    - 相关 Commit：`2cdbb814`, `b9080169`, `4536ec1a`, `ef927534`
  - [ssh-password-credential-sync.md](../archive/bugfix/remote-direct-run/ssh-password-credential-sync.md) - SSH 密码管理与自动提权架构重构
    - 状态：已完成（2026-06-15）
    - 目标：修复远端 SSH 密码保留和 sudo 自动提权失败
    - 相关 Commit：`0a75a3cb`, `5453839b`
  - [README.md](../archive/bugfix/remote-direct-run/README.md) - 专题索引与演进历史

**归档位置**: `docs/dev/archive/bugfix/remote-direct-run/`

**归档原因**: 功能已完整实现并通过验证，相关代码已落盘。文档保留供未来参考和演进追踪。

---

## 文档规范

### 必需章节

1. **问题背景** - 为什么需要这个设计
2. **设计目标** - 要达成什么效果
3. **架构设计** - 模块划分、数据流图
4. **核心决策** - 关键设计决策与理由
5. **实现优先级** - P0/P1/P2 分级
6. **关键文件清单** - 新增/修改哪些文件
7. **风险与注意事项** - 已知风险和缓解措施

### 可选章节

- 用户体验流程（有 UI 交互时）
- 参考资料（对比旧实现或外部资料）
- 数据结构设计（复杂数据建模时）

### 文档生命周期

1. **设计中** - 方案讨论阶段，可能调整
2. **实施中** - 正在编码实现
3. **已完成** - 功能已上线
4. **已废弃** - 方案被替换或功能下线

### 归档原则

- 功能完整实现并通过验证后，相关架构文档应归档至 `docs/dev/archive/<category>/`
- 归档时保留专题目录结构，便于关联文档集中管理
- 在本 README 中保留归档索引和简要说明
- 归档文档不再更新（除非有重大演进需要同步）

---

## 如何添加新文档

1. 在本目录创建 `<feature-name>.md`
2. 按照上述规范编写章节
3. 更新本 README 的目录索引
4. 提交到 git（如需进 git）或保留在本地（内部文档）
5. 功能完成后，考虑是否需要归档至 `docs/dev/archive/`
