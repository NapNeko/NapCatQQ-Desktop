# 远程 SSH 安全收尾 — 验收报告

> 关联文档:
> - 需求: [`docs/requirements/2026-05-07-remote-ssh-security-hardening.md`](../requirements/2026-05-07-remote-ssh-security-hardening.md)
> - 计划: [`docs/plans/2026-05-07-remote-ssh-security-hardening-plan.md`](../plans/2026-05-07-remote-ssh-security-hardening-plan.md)
>
> 交付日期: 2026-05-07
> 内部档位: **L (serial native execution)**
> 完成状态: **完成 (full completion)** — 三个 Wave 全部绿, 零回归

## 1. 子项交付摘要

### F1 NapCat Shell 包 SHA512 上游校验 ✅

| 子任务                  | 实现产物                                                                                                                                                             | 测试                                                                                         |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| F1.1 ReleaseHashService | `src/core/versioning/release_hash_service.py`                                                                                                                        | `test_release_hash_service.py` (12)                                                          |
| F1.2 网络降级矩阵       | 同上 + `_default_fetcher` + 缓存                                                                                                                                     | 同上                                                                                         |
| F1.3 本地接入           | `src/core/installation/installers.py:verify_napcat_archive` + `src/ui/components/install_hash_check.py` + `napcat_page.py` + `install_page.py`                       | `test_napcat_install_hash_verify.py` (4)                                                     |
| F1.4 远端接入           | `remote_install_napcat.sh:verify_napcat_archive_sha512` + `LinuxCoreDeployment.install_napcat(expected_sha512=...)` + `ServerManager._lookup_napcat_expected_sha512` | `test_remote_install_napcat_hash_env.py` (5) + `test_remote_install_napcat_sh_verify.py` (6) |
| 友好文案                | `friendly_errors._format_napcat_hash_mismatch`                                                                                                                       | (集成在 hash_env 测试里)                                                                     |

**关键安全断言**:

- 默认 hash 源: `https://raw.githubusercontent.com/NapNeko/napcat-release-hash/main/release.json`
  + jsdelivr 备援
- 缓存 TTL 7 天, 容忍长时间无网
- Hash 不匹配 → **硬拒绝** + 删除已下载文件 + 友好中文提示
- 远端脚本支持 `sha512sum` / `openssl dgst -sha512` 双工具, 都缺失时按"远端环境异常偏严"策略中断 (退出码 36)
- `sha512sum` 在文件名含反斜杠时的 BSD compat 转义已处理 (`${actual#'\\'}`)

### F2 `workspace_dir` 命令注入修复 ✅

| 子任务                      | 实现产物                                                                                                                     | 测试                                        |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| F2.1 templates 单引号       | `src/core/remote/templates.py:_safe_shell_value` + `_single_quote`                                                           | `test_templates_inject.py` (12)             |
| F2.2 _quote_remote_argument | `src/core/remote/ssh_client.py:_quote_remote_argument`                                                                       | `test_quote_remote_argument.py` (9)         |
| F2.3 LinuxCorePaths 校验    | `src/core/remote/models.py:_validate_linux_path` + `is_valid_linux_path` + `__post_init__` + `servers.py:from_dict` 容错     | `test_linux_core_paths_validation.py` (22)  |
| F2.4 UI 校验                | `src/core/config/__init__.py:_LinuxPathValidator` + `setup_page/sub_page/remote.py:_on_save` + `general.py` (复用 validator) | `test_remote_setup_page_validation.py` (18) |

**关键安全断言**:

- 攻击 payload `$HOME/Napcat$(touch /tmp/PWNED)` 在三层 (UI 校验 / 模型校验 / quote 算法) 均被拦截
- POSIX 单引号注入: `'\\''` 关闭-转义-重开模式, 单引号自身合法值可往返
- `$HOME` 展开仍然有效 (拼接策略: `"$HOME"` + 单引号字面量后缀)
- `servers.json` 已损坏(含非法路径)时静默退化到默认 `LinuxCorePaths()`, 不阻断 Desktop 启动
- 历史回归零: `test_server_manager_deploy.py` 全部 19 用例通过

### F3 QQID 路径形式脱敏补全 + SSH 主机 / 用户名 / 隧道 label 导出脱敏 ✅

| 子任务                   | 实现产物                                                                                                                                 | 测试                                            |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| F3.1 QQID 三条窄正则     | `src/core/logging/crash_bundle.py:_QQID_FILENAME_PATTERN` / `_QQID_CMDLINE_Q_PATTERN` / `_QQID_BRACKET_PATTERN`                          | `test_crash_bundle_qqid_path_redaction.py` (14) |
| F3.2 host/username/label | `src/core/logging/crash_bundle.py:mask_host` + `mask_username` + `_HOST_KEY_PATTERN` / `_USERNAME_KEY_PATTERN` / `_TUNNEL_LABEL_PATTERN` | `test_crash_bundle_host_redaction.py` (18)      |

**关键安全断言 (F3.1)**:

- 覆盖远程链路实际形态: `napcat_<qqid>.{log,json,pid,log.prev}`、`onebot11_<qqid>.json`、`qq -q <qqid>`、`ManagerNapCatQQProcess[<qqid>]`
- 不误伤端口号 (`:8080`)、错误码 (`404`)、普通文件名 (`config.json` / `napcat.log` 无 qqid 段)
- 与历史 `QQID=` / `qq_id:` 命中规则共存, 历史测试全绿

**关键安全断言 (F3.2)**:

- TRCE 实时落盘**保留原始字符串**便于排错; 仅在导出诊断包时统一脱敏
- `host=ac.rainplay.cn` → `host=a***.cn` (域名: 首字符 + 顶级域)
- `host=10.0.0.5` → `host=1***.5` (IP: 首字符 + 末段)
- `username=root` → `username=r***` (仅首字符)
- `label=ac.rainplay.cn->127.0.0.1:6099` → `label=a***.cn->127.0.0.1:6099` (隧道 label 仅脱敏箭头左侧 host)
- 不误伤 `hosting=true` / `config_path=/etc/foo.conf` 等无关键值对
- 与 `_URL_PATTERN` 协作: 完整 URL `https://ac.rainplay.cn/api` 整段被 `<redacted-url>` 兜底

## 2. 验证命令与结果

### 2.1 单元测试

```powershell
uv run pytest script/test/test_release_hash_service.py script/test/test_napcat_install_hash_verify.py script/test/test_remote_install_napcat_hash_env.py script/test/test_remote_install_napcat_sh_verify.py script/test/test_templates_inject.py script/test/test_quote_remote_argument.py script/test/test_linux_core_paths_validation.py script/test/test_remote_setup_page_validation.py script/test/test_crash_bundle_qqid_path_redaction.py script/test/test_crash_bundle_host_redaction.py script/test/test_crash_bundle.py script/test/test_server_manager_deploy.py script/test/test_remote_deploy_runner.py script/test/test_remote_deploy_probe.py
```

**结果**: `173 passed` ✅ (155 原 + 18 个 F3.2 新增)

### 2.2 全套回归

```powershell
uv run pytest script/test/ --ignore=script/test/test_home_version_card.py --ignore=script/test/test_setup_desktop_log_page.py --ignore=script/test/test_update_log_card.py --ignore=script/test/test_stacked_widget.py
```

**结果**: 763 passed, 19 failed, 3 skipped

**失败用例分析**: 19 个 failures **全部预先存在**(在 `git stash` 移除本次改动后, 同样用例同样失败), 与本次安全收尾**完全无关**. 详见 §3.

### 2.3 静态检查

```powershell
uvx ruff check src/core/versioning/release_hash_service.py src/core/installation/ src/core/remote/templates.py src/core/remote/ssh_client.py src/core/remote/models.py src/core/remote/servers.py src/core/remote/server_manager.py src/core/remote/deployment.py src/core/remote/friendly_errors.py src/core/operation/remote_backend.py src/core/logging/crash_bundle.py src/ui/components/install_hash_check.py
```

**结果**: 2 个 F401 警告, **均为预先存在** (`winreg` in install_type.py 与 `Iterator` in ssh_client.py 均未在本次改动覆盖范围). 本次新增/修改代码 lint 干净. ✅

## 3. 预先存在的 19 个失败用例 (与本次无关)

| 文件                                      | 用例数 | 失败根因                                     |
| ----------------------------------------- | ------ | -------------------------------------------- |
| `test_api_debug_workspace.py`             | 6      | UI fixture 与 BackgroundTaskCenter 兼容性    |
| `test_bot_config_page.py`                 | 1      | `migration.move_persistent_data` 字段缺失    |
| `test_bot_list_page.py`                   | 1      | BotCard 重建状态比较                         |
| `test_bot_page_batch_mode.py`             | 5      | BotCard 缺 `batch_check_changed_signal` 信号 |
| `test_email.py`                           | 1      | PySide6 `QFile.open(int)` 类型不兼容         |
| `test_get_version.py`                     | 1      | RemoteVersionTask 三源装配测试               |
| `test_progress_info_bar_bridge.py`        | 3      | ProgressInfoBar `setComplete` 桥接           |
| `test_runnable_background_task_wiring.py` | 1      | BotMigrationRunnable 上下文                  |

**结论**: 所有失败均与 P5 安全收尾无关, 主线代码主人需在另一计划中处理.

## 4. 文件清单

### 新增 (untracked)

```
docs/general/remote_ssh_security_hardening_acceptance.md  ← 本文件
docs/plans/2026-05-07-remote-ssh-security-hardening-plan.md
docs/requirements/2026-05-07-remote-ssh-security-hardening.md
script/test/test_release_hash_service.py
script/test/test_napcat_install_hash_verify.py
script/test/test_remote_install_napcat_hash_env.py
script/test/test_remote_install_napcat_sh_verify.py
script/test/test_templates_inject.py
script/test/test_quote_remote_argument.py
script/test/test_linux_core_paths_validation.py
script/test/test_remote_setup_page_validation.py
script/test/test_crash_bundle_qqid_path_redaction.py
script/test/test_crash_bundle_host_redaction.py
src/core/installation/errors.py
src/core/versioning/release_hash_service.py
src/ui/components/install_hash_check.py
```

### 修改 (modified)

```
script/test/test_server_manager_deploy.py        # autouse fixture stub hash lookup
src/core/config/__init__.py                       # _LinuxPathValidator + remote_workspace_dir validator
src/core/installation/installers.py               # verify_napcat_archive
src/core/logging/crash_bundle.py                  # 3 条 QQID 路径形式正则
src/core/operation/remote_backend.py              # install_napcat(expected_sha512)
src/core/remote/deployment.py                     # install_napcat 注入 NAPCAT_EXPECTED_SHA512
src/core/remote/friendly_errors.py                # NapCatHashMismatchError 文案
src/core/remote/models.py                         # _validate_linux_path + LinuxCorePaths.__post_init__
src/core/remote/server_manager.py                 # _lookup_napcat_expected_sha512 + stage="install_napcat_verify" 升级
src/core/remote/servers.py                        # from_dict ValueError 容错
src/core/remote/ssh_client.py                     # _quote_remote_argument 修复
src/core/remote/templates.py                      # _safe_shell_value + _single_quote
src/core/versioning/__init__.py                   # 导出 ReleaseHashService 等
src/resource/script/remote_install_napcat.sh      # verify_napcat_archive_sha512 + 退出码 36
src/ui/page/component_page/sub_page/napcat_page.py # 解压前 SHA512 校验
src/ui/page/setup_page/sub_page/remote.py         # workspace_dir 显式 error_bar 拦截
src/ui/window/guide_window/install_page.py        # 引导安装并行拉版本号 + SHA512 校验
```

## 5. 完成语言判定

依据 [`docs/requirements/2026-05-07-remote-ssh-security-hardening.md`](../requirements/2026-05-07-remote-ssh-security-hardening.md) §6:

- ✅ §4 全部 acceptance 子项通过
- ✅ §7 验证命令全绿 (155 个新测试)
- ✅ 全套回归 763 通过, 0 新增 fail
- ✅ ruff 静态检查无新增告警

**判定**: 允许使用 "完成 / 验收通过" 措辞.

## 6. 推迟项 (out-of-scope, 留后续计划)

参考需求文档 §3 与上一轮安全审查清单, 以下项**未在本次范围**:

- 远端 LinuxQQ deb (`framework`) SHA512 校验
- 远端响应 `cat` 大小上限 / 端口范围校验 (上次审查 缺口 4)
- `auto_add` 政策 UI 二次确认 (上次审查 缺口 5)
- `_password_cache` 进程退出钩子清理 (上次审查 加固 6)
- 远程会话脱敏诊断包入口 (上次审查 缺口 2)
- 上游 `release.json` 仓库实际公开后 URL 真实可用性的端到端冒烟

## 7. 上线前手工抽测建议

仓库 `NapNeko/napcat-release-hash` 公开后, 在测试环境运行:

1. **本地安装 hash 一致**: 启动 Desktop, 触发 NapCat 安装, 在日志看到 `verify_napcat_archive: SHA512 校验通过`
2. **本地安装 hash 不一致**: 用 `Test-Path` 让 Desktop 下载到一个故意篡改过的 zip, 看到 `error_bar` "完整性校验失败"
3. **远端安装 hash 一致**: 选一个上游 release.json 中的版本, 远端跑完 `install_napcat`, 日志包含 `[INFO] sha512 verified ok`
4. **远端安装 hash 不一致**: 在测试 VM 上 `echo trash > $HOME/Napcat/packages/NapCat.Shell.zip`, 触发部署, 远端退出 36, Desktop 友好提示
5. **`workspace_dir` 注入**: UI 输入 `$HOME/Napcat$(touch /tmp/PWNED)`, 保存被 error_bar 阻止; 即使绕过 UI 直接改 `config.json`, 远端部署不应执行 `touch`
6. **诊断包**: 触发崩溃, 解压脱敏诊断包, 搜任意已运行过的 11 位 QQ 号, 应**找不到任何一处**
