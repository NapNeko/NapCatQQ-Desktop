# SSH 远程支持边界扩展 — 需求冻结

- **创建日期**：2026-05-08
- **冻结时间戳**：governed by `vibe` runtime
- **会话主题**：把 SSH 远程从「仅 Ubuntu」扩到「Debian/Ubuntu + RHEL 系」并完善前置体检

---

## 1. 目标 (Goal)

把现有 SSH 远程部署能力的**官方支持边界**从「仅 Ubuntu 24」扩展到主流 Debian 系 + RHEL 系
发行版，**不引入新的 UI 步骤**，通过 deploy 流程内的前置兼容性体检 + 集中式
依赖矩阵让分支决策从脚本硬编码上移到 Python 数据模块。

## 2. 交付物 (Deliverable)

1. `src/core/remote/distro_matrix.py` — 集中式发行版数据模块（family / package_manager /
   display_name / support_tier）
2. `src/core/remote/deployment.py` — `LinuxCoreDeploymentProbe` 增加 `family`、
   `compat_status`、`incompatibility_reason` 计算属性；新增 `evaluate_compatibility()`
   返回结构化体检报告
3. `src/core/remote/server_manager.py` — `deploy_server` 在 install_qq 之前先跑
   兼容性体检，把人话报告 emit 到 `deployment_log`，发现硬不兼容（无 dpkg/无 rpm2cpio
   且无法装；非 amd64/arm64）时**提前**抛 `RemoteDeploymentError(stage="preflight")`
4. `src/core/remote/friendly_errors.py` — 新增 preflight stage 的友好文案
5. `src/resource/script/remote_install_linuxqq.sh` — RHEL 系依赖列表里前置 `epel-release`
   bootstrap（CentOS/Rocky 最小化镜像缺 `xorg-x11-server-Xvfb`），把 `apt-get` 路径里
   的 `rpm2cpio` 包名修正为 `rpm2cpio` 不存在则降级到不安装（Debian/Ubuntu 上 `rpm2cpio`
   实际由 `rpm` 包提供，不是必需）
6. `src/ui/page/remote_page/__init__.py` — 「远程功能使用前提示」文案重写，明确
   支持的发行版与架构清单；移除 Ubuntu-only 措辞
7. `src/ui/page/remote_page/server_edit_dialog.py` — 用户名 placeholder 去 ubuntu 化
8. `script/test/test_remote_distro_matrix.py` — 矩阵 + probe 兼容性评估单元测试
9. `script/test/test_remote_preflight.py` — `deploy_server` 前置体检的失败/通过路径测试

## 3. 约束 (Constraints)

- **不**新增 UI 页面 / 不新增对话框；体检结果只走 `deployment_log` 信号 + 失败 InfoBar
- **不**改 SSH 协议层 / paramiko 调用方式
- **不**触碰 NapCat 安装阶段的脚本（`remote_install_napcat.sh`）逻辑
- **不**改 `LinuxCorePaths` 字段（路径布局保持 P5 安全收尾后的版本）
- **保留** `force_*` 系列开关；本期不引入 `force_unsupported_distro` 开关
- **保留** Ubuntu 24 的实测覆盖；新增发行版**仅**通过 mock 单测保证分发逻辑正确，不要求
  真起容器
- 脚本侧改动必须**幂等**且**可被 P5 单引号注入语法消费**（不引入 `$()` / 反引号）

## 4. 验收标准 (Acceptance Criteria)

### 4.1 代码可执行性

- [ ] `python -c "from src.core.remote.distro_matrix import KNOWN_DISTROS; print(len(KNOWN_DISTROS))"` 输出 ≥ 6
- [ ] `python -c "from src.core.remote.deployment import LinuxCoreDeploymentProbe"` 不报错
- [ ] `LinuxCoreDeploymentProbe.compat_status` 在 ubuntu/debian/centos/rhel/rocky/almalinux/fedora
  上返回 `"supported"`；在 arch/alpine/openSUSE 上返回 `"unsupported"`；
  在 `distro_id=None` 但 `has_dpkg=True` 时返回 `"unknown_but_runnable"`

### 4.2 单元测试

- [ ] `pytest script/test/test_remote_distro_matrix.py -q` 全绿
- [ ] `pytest script/test/test_remote_preflight.py -q` 全绿
- [ ] 现有 `test_remote_*` 套件**不**回归（特别是 `test_server_manager_*` /
  `test_remote_deployment_*`）

### 4.3 行为变更（可代码 review 验证）

- [ ] `RemoteUsageNoticeBox` 文案不再含 "仅支持 Ubuntu" 字样
- [ ] `deploy_server` 在 `install_qq` 之前会调用一次 `evaluate_compatibility()` 并
  emit 至少一条 `deployment_log` 含 `[PREFLIGHT]` 前缀
- [ ] 在 mock probe 返回 `compat_status="unsupported"` 时，`deploy_server` 抛
  `RemoteDeploymentError(stage="preflight")` 且**不**调用 `install_qq`/`install_napcat`

### 4.4 产品验收 (Product Acceptance)

- 在已有 Ubuntu 24 服务器上跑现有部署流程，结果与之前**一致**（部署日志多一段
  preflight 输出，不影响成败）
- 文案 review：远程页面不再让用户产生「我用 CentOS 是不是不能用」的疑虑

## 5. 手工抽查 (Manual Spot Checks)

- 启动 Desktop，进入远程页面 → 首次「远程功能使用前提示」文字应反映新边界
- 用 `ServerEditDialog` 添加任意服务器 → 用户名输入框 placeholder 不再是 "ubuntu"
- 在 grep 结果里搜 "仅支持 Ubuntu" / "目前仅支持" 应**只**剩历史归档文档命中，
  `src/` 与 `docs/` 现行文档不再命中

## 6. 完成语言策略 (Completion Language Policy)

只有当 4.1+4.2+4.3 全部勾选完成、phase_cleanup 报告写入 outputs 后，才允许
使用「全部完成 / 已交付 / 验收通过」等完成性措辞。在此之前对外都用阶段性措辞
（例如「P1 完成，等待跑完 4.2 单元测试」）。

## 7. 交付真相契约 (Delivery Truth Contract)

- **可声明完成**的依据：`pytest script/test/test_remote_distro_matrix.py
  script/test/test_remote_preflight.py -q` 退出码 0
- **不可代替**的人工验收点：在真实 Ubuntu 24 服务器上的旧路径回归（开发者自验）
- **未达成必须明示**：任一新增测试未跑、未跑 / 跑挂均不得用「完成」措辞

## 8. 非目标 (Non-Goals)

- Arch Linux (pacman) 支持
- openSUSE (zypper) 支持
- Termux / proot-distro 支持
- LiteLoaderQQNT framework 模式（`install.framework.sh`）
- Docker 化部署（`example/new/installer.py`）
- 在 UI 上新增「兼容性体检」独立页面（明确不做）
- 引入「自担风险强制部署不支持发行版」开关

## 9. 自治模式 (Autonomy Mode)

`interactive_governed`。XL 计划阶段的 wave/batch 边界处不再向用户提问；
只有出现「破坏冻结需求 / 需要新建第二个交付目标」时才回到用户。

## 10. 推断假设 (Inferred Assumptions)

- 用户没有 RHEL 系真实测试环境，本期 RHEL 系支持以 `dpkg/rpm2cpio` 探测分支正确性 +
  脚本路径 dnf 兜底为准；不要求开发者跑通 CentOS 实机
- "保留 deploy 一键不引入新 UI 步骤" 与 "兼容性体检" 的并集解读为：体检逻辑跑在
  现有 deploy 流程里，结果走 `deployment_log` 信号；UI 端可在后续单独做可视化
- 脚本里现有的 `dnf install ... rpm2cpio cpio xorg-x11-server-Xvfb ...` 在
  CentOS Stream 9 / Rocky 9 minimal 上需要 epel；本期一并把 epel bootstrap 加上
- 现有所有 `test_remote_*.py` 在不改动 deploy_server 与 probe 公共签名时**不会**回归
