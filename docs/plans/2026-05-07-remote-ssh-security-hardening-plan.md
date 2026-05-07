# 远程 SSH 安全收尾 — 执行计划 (xl_plan)

> 关联需求: [`docs/requirements/2026-05-07-remote-ssh-security-hardening.md`](../requirements/2026-05-07-remote-ssh-security-hardening.md)
>
> 内部档位: **L (serial native execution)**
> 执行模式: 三个 Wave 串行, 每个 Wave 内部测试驱动, Wave 间不做并行.
> 终止阶段: `phase_cleanup`

## 0. 档位决定 (internal grade decision)

- 子项数 = 3, 互相**有耦合** (Wave 1 改 `_build_script_variables` 间接影响 Wave 2
  的 `inject_script_variables` 输出形态).
- 工作量估算: 单 agent 串行 ≈ 4 小时, XL 并行受限于耦合.
- 决定: **L 档**, native serial; 不 spawn 子代理.

## 1. Wave 结构

| Wave | 子项 | 可独立验证 | 依赖前序 |
| --- | --- | --- | --- |
| W1 | F1 SHA512 上游校验 | ✅ 自带单测 | 无 |
| W2 | F2 workspace_dir 注入修复 | ✅ 自带单测 | 部分依赖 W1 (脚本注入语法变更影响 W1 测试) |
| W3 | F3 QQID 脱敏补全 | ✅ 自带单测 | 无 |
| Verify | 全套 pytest 回归 | 必须最后 | W1+W2+W3 完成 |
| Cleanup | phase_cleanup 报告 | — | Verify 通过 |

## 2. Ownership 与写入边界

| Wave | 允许写入路径 | 禁止改动 |
| --- | --- | --- |
| W1 | `src/core/versioning/release_hash_service.py` (新建)<br>`src/core/installation/installers.py`<br>`src/core/remote/deployment.py`<br>`src/core/remote/errors.py`<br>`src/resource/script/remote_install_napcat.sh`<br>`src/ui/window/guide_window/install_page.py`<br>`src/ui/page/component_page/sub_page/napcat_page.py`<br>`script/test/test_release_hash_service.py` (新建)<br>`script/test/test_napcat_install_hash_verify.py` (新建)<br>`script/test/test_remote_install_napcat_hash_env.py` (新建)<br>`script/test/test_remote_install_napcat_sh_verify.py` (新建) | `src/core/remote/templates.py` (W2 边界) |
| W2 | `src/core/remote/templates.py`<br>`src/core/remote/ssh_client.py` (`_quote_remote_argument`)<br>`src/core/remote/models.py` (`LinuxCorePaths.__post_init__`)<br>`src/core/remote/servers.py` (`from_dict` 容错)<br>`src/core/remote/deployment.py` (`_build_script_variables` 注入 `$HOME` 展开值)<br>`src/ui/page/setup_page/sub_page/remote.py`<br>`src/ui/page/setup_page/sub_page/general.py`<br>`script/test/test_templates_inject.py` (新建)<br>`script/test/test_quote_remote_argument.py` (新建)<br>`script/test/test_linux_core_paths_validation.py` (新建)<br>`script/test/test_remote_setup_page_validation.py` (新建) | `release_hash_service.py` (W1 边界) |
| W3 | `src/core/logging/crash_bundle.py`<br>`script/test/test_crash_bundle_qqid_path_redaction.py` (新建)<br>`script/test/test_crash_bundle.py` (补 case) | 任何远程模块 |

## 3. Wave 1 执行步骤 — F1 SHA512

### W1.1 模型与服务骨架 (TDD)

1. 写 `script/test/test_release_hash_service.py` (~10 个 case):
   - parse 单条 entry / 完整列表
   - 缓存写入与读取
   - 多源 fallback (mock urllib)
   - 损坏 JSON 不抛
   - `lookup("v4.18.1")` 与 `lookup("4.18.1")` 等价
2. 实现 `src/core/versioning/release_hash_service.py`:
   - `ReleaseHashEntry` dataclass
   - `ReleaseHashFetchResult` enum (FETCHED / CACHED / NONE)
   - `ReleaseHashService` 类: `fetch` / `lookup` / `cache_path` / `is_cache_fresh`
   - 用 stdlib `urllib.request` (项目已有的 `httpx` 也行, 看 `requirements.txt`)
   - 单源超时: connect=5s, read=15s
3. 跑 `uv run pytest script/test/test_release_hash_service.py -v` 直至全绿.

### W1.2 错误类型扩展

1. 在 `src/core/remote/errors.py` 新增:
   - `NapCatHashMismatchError(RemoteDeploymentError)` (本地也用, 类名不带 Remote 前缀更合适, 改为通用):
     ```python
     class NapCatHashMismatchError(Exception):
         def __init__(self, version: str, expected: str, actual: str, file: str) -> None: ...
     ```
   - 放在 `src/core/installation/errors.py` 更合适 — 决定: 新建该文件并 export.
2. `friendly_errors._RAW_REGISTRY` 增加该类型的中文映射.

### W1.3 本地安装路径接入

1. 写 `script/test/test_napcat_install_hash_verify.py`:
   - mock `ReleaseHashService.lookup` 返回已知 SHA512
   - 用真实小文件 + 真实 sha512 测一致 / 不一致两条路径
   - 不一致后 zip 文件被删除
2. 修改 `src/core/installation/installers.py`:
   - `NapCatInstall` 增加 `verify_archive(version, archive_path, hash_service)` 方法
   - 解压前调用; 抛 `NapCatHashMismatchError` 时上层捕获
3. 修改 UI 入口 `install_page.py` / `napcat_page.py`:
   - 在 `download_finish_signal` 回调里, 先尝试 `hash_service.lookup`, 走降级矩阵
   - hash 缺失时: `MessageBox` 二次确认 → 用户选"继续" → 正常解压, 用户选"取消" → 删除 zip
   - hash 不一致: `error_bar` + 删除 zip

### W1.4 远端安装路径接入

1. 修改 `src/resource/script/remote_install_napcat.sh`:
   - `download_file` 之后插入 §4.F1.4 的校验块
   - 退出码 36
2. 写 `script/test/test_remote_install_napcat_sh_verify.py`:
   - 用 `bash` 跑脚本片段(提取关键段做 `source`); 一致返回 0 / 不一致返回 36
   - Windows 跑测试时跳过 (skipif sys.platform == "win32" and shutil.which("bash") is None)
3. 修改 `src/core/remote/deployment.py` `install_napcat`:
   - `ReleaseHashService.lookup(version_to_install)` 取 hash
   - 注入到 `env_parts.append(f'NAPCAT_EXPECTED_SHA512={shlex.quote(hash_value)}')`
   - 退出码 36 → 抛 `RemoteDeploymentError(stage="install_napcat_verify")`
4. 写 `script/test/test_remote_install_napcat_hash_env.py`:
   - 验证命令字符串拼装包含 expected hash
   - 验证退出码 36 转换为 `RemoteDeploymentError`

### W1.5 W1 验收

```powershell
uv run pytest script/test/test_release_hash_service.py script/test/test_napcat_install_hash_verify.py script/test/test_remote_install_napcat_hash_env.py script/test/test_remote_install_napcat_sh_verify.py -v
uv run ruff check src/core/versioning/release_hash_service.py src/core/installation/ src/core/remote/deployment.py src/core/remote/errors.py
```

W1 全绿后才能进 W2.

## 4. Wave 2 执行步骤 — F2 注入修复

### W2.1 templates 单引号注入 (TDD)

1. 写 `script/test/test_templates_inject.py`:
   - 普通字符串
   - 含 `$()` / 反引号
   - 含单引号
   - bash subprocess 真实跑, 验证变量取出值
2. 重写 `src/core/remote/templates.py` `inject_script_variables`:
   ```python
   def _shell_single_quote(value: str) -> str:
       return "'" + value.replace("'", "'\\''") + "'"
   ```
3. **重要副作用**: 历史行为是 `key="$HOME/Napcat"` 让 bash 自动展开 `$HOME`.
   切到单引号后**不再展开**. 必须在 `_build_script_variables` 阶段就把
   `$HOME` 展开为 SSH 探测到的实际 home 目录:
   - 复用 `SSHClient._get_remote_home_directory` 的探测结果
   - 把 `paths.workspace_dir` 中的 `$HOME` 替换为该值, 其他 `$HOME` 字段同理
4. 写 `script/test/test_deployment_paths_home_expansion.py` (可选, 嵌入到现有
   `test_remote_deployment_install_napcat.py`).
5. 跑 W2.1 测试至绿.

### W2.2 `_quote_remote_argument` 修复

1. 写 `script/test/test_quote_remote_argument.py`.
2. 修改 `src/core/remote/ssh_client.py`:
   ```python
   @staticmethod
   def _quote_remote_argument(value: str) -> str:
       if value.startswith("$HOME"):
           suffix = value[len("$HOME"):]
           return '"$HOME"' + (shlex.quote(suffix) if suffix else "")
       return shlex.quote(value)
   ```
3. 跑测试至绿.

### W2.3 `LinuxCorePaths` 校验

1. 写 `script/test/test_linux_core_paths_validation.py`.
2. 在 `src/core/remote/models.py` 加 `__post_init__` + 正则.
3. 修改 `src/core/remote/servers.py` `ServerProfile.from_dict`: 捕获 `ValueError`
   退化到默认 paths 并 `logger.warning`.
4. 跑测试至绿.

### W2.4 UI 同源校验

1. 写 `script/test/test_remote_setup_page_validation.py` (Qt offscreen).
2. 修改 `src/ui/page/setup_page/sub_page/remote.py` 与 `general.py`:
   - 抽出公用 `_validate_workspace_dir(value: str) -> str | None` 返回错误描述
   - `save` 回调中调用, 错误时 `error_bar` 拒绝保存
3. 跑测试至绿.

### W2.5 W2 验收

```powershell
uv run pytest script/test/test_templates_inject.py script/test/test_quote_remote_argument.py script/test/test_linux_core_paths_validation.py script/test/test_remote_setup_page_validation.py -v
uv run pytest script/test/test_remote_deployment_install_napcat.py -v   # 回归
uv run ruff check src/core/remote/templates.py src/core/remote/ssh_client.py src/core/remote/models.py src/core/remote/servers.py src/core/remote/deployment.py src/ui/page/setup_page/sub_page/
```

## 5. Wave 3 执行步骤 — F3 QQID 脱敏补全

### W3.1 (TDD)

1. 写 `script/test/test_crash_bundle_qqid_path_redaction.py` (4 个新 case + 边界).
2. 修改 `src/core/logging/crash_bundle.py`:
   - 新增三条窄正则 (`_QQID_FILENAME_PATTERN` / `_QQID_CMDLINE_PATTERN` /
     `_QQID_BRACKET_PATTERN`)
   - 在 `sanitize_text_for_export` 中按"先窄后宽"顺序应用
3. 跑测试至绿.

### W3.2 W3 验收

```powershell
uv run pytest script/test/test_crash_bundle.py script/test/test_crash_bundle_qqid_path_redaction.py -v
uv run ruff check src/core/logging/crash_bundle.py
```

## 6. Verify (全套回归)

```powershell
uv run pytest script/test/ -x
```

要求: **零新增 fail / error**, 跳过的测试与 P4 验收时一致.

如果出现 P4 时绿、本次红的测试, 必须先回滚 / 修复, 不能继续 phase_cleanup.

## 7. 完成语言规则 (completion language rules)

- W1 / W2 / W3 任一未通过 → 报告中只能写 "已交付 N/3"
- Verify 全绿 → "完成"
- Verify 出现回归且未修复 → "阻塞, 请审阅 <文件>"

## 8. 回滚规则 (rollback rules)

- W1 失败回滚: `git checkout -- src/core/versioning/release_hash_service.py
  src/core/installation/ src/core/remote/deployment.py src/core/remote/errors.py
  src/resource/script/remote_install_napcat.sh src/ui/window/guide_window/install_page.py
  src/ui/page/component_page/sub_page/napcat_page.py`
  并删除新增测试文件.
- W2 失败回滚: 同理 git checkout 涉及文件.
- W3 失败回滚: 同理.
- 跨 Wave 已完成的可保留 (互相独立, 测试单独覆盖).

## 9. Phase Cleanup 期望

phase_cleanup 阶段必须留下:

- 单一交付报告 `docs/general/remote_ssh_security_hardening_acceptance.md`,
  包含: 各 acceptance 条目对应实测命令 + 输出摘要 / 是否通过 / 残留项.
- 三个 Wave 各自一条 git commit (内容范围与 §2 对齐).
- 临时文件清理: 无 (本计划不产生临时大文件).
- pytest 缓存与 ruff 缓存不动.

## 10. 风险与应对

| 风险 | 概率 | 应对 |
| --- | --- | --- |
| `napcat-release-hash` 仓库当前 404 | 高 | 把 URL 设为常量, 测试用 mock + 本地文件; 上线前最终核对真实 URL. |
| jsdelivr CDN 在某些大陆网络也慢 | 中 | 缓存 7 天 TTL 容忍长时间无网, 二次确认对话框托底. |
| 远端 `sha512sum` 缺失 | 低 | 退出码 36 + 友好文案提示, 不静默通过. |
| 单引号转义后旧脚本变量取不到值 | 中 | W2.1 步骤 3 在 Python 端预先展开 `$HOME`; 测试覆盖 home 探测失败的兜底. |
| `LinuxCorePaths` 严格化导致老 `servers.json` 加载失败 | 中 | `from_dict` 容错 + warning, 不阻断 Desktop 启动. |
| QQID 正则误伤普通文件名 | 低 | 严格 `\d{5,12}` 长度 + 上下文锚点, 测试覆盖 4 位以下/普通文件名. |

## 11. 验证矩阵 (delivery acceptance plan)

| Acceptance | 验证方式 | 期望产出 |
| --- | --- | --- |
| F1.1 模型与服务 | `pytest test_release_hash_service.py` | 全绿 |
| F1.2 降级矩阵 | mock 网络场景 + 缓存场景 | 全绿 |
| F1.3 本地接入 | `pytest test_napcat_install_hash_verify.py` | 全绿 |
| F1.4 远端接入 | `pytest test_remote_install_napcat_hash_env.py` + `test_remote_install_napcat_sh_verify.py` | 全绿 |
| F2.1 注入修复 | `pytest test_templates_inject.py` (含真实 bash subprocess) | 全绿 |
| F2.2 quote_remote_argument | `pytest test_quote_remote_argument.py` | 全绿 |
| F2.3 LinuxCorePaths 校验 | `pytest test_linux_core_paths_validation.py` | 全绿 |
| F2.4 UI 校验 | `pytest test_remote_setup_page_validation.py` | 全绿 |
| F3 QQID 脱敏 | `pytest test_crash_bundle_qqid_path_redaction.py` | 全绿 |
| 回归 | `pytest script/test/ -x` | 与 P4 验收同等绿 |
