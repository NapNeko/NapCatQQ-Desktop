# P0 验收清单 — 操作抽象层 + 服务器管理

> 对应 [`docs/general/remote_ssh_plan.md`](./remote_ssh_plan.md) §7 的 **P0** 阶段。
> 验收范围：能添加服务器、测试 SSH 连接、持久化服务器配置；底层 `OperationBackend` 抽象就绪，
> 进程 / 安装 / 日志 / WebUI 高层方法接口已定义但实现排期至 P1 / P2。

## 1. 交付物

### 1.1 新增模块

| 路径 | 职责 |
| --- | --- |
| `src/core/operation/__init__.py` | 延迟导入 facade |
| `src/core/operation/backend.py` | `OperationBackend` ABC + 数据模型（`FileEntry` / `ProcessStatus` / `WebUIEndpoint` / `InstallationInfo` / `ProgressCallback`） |
| `src/core/operation/local_backend.py` | `LocalBackend` 本地实现（文件 8 op + 检测 3 op 完整；进程 / 安装写入 / 日志 / WebUI 留 `NotImplementedError` 标注 P2） |
| `src/core/operation/remote_backend.py` | `RemoteBackend` 骨架（文件 8 op + 检测 3 op + 进程查询 + 日志 完整；启停 / 安装写入 / WebUI 留 `NotImplementedError` 标注 P1/P2） |
| `src/core/remote/servers.py` | `ServerProfile` + `ServerRegistry`（密码不落盘，原子写） |
| `src/core/remote/server_manager.py` | `ServerManager`（多服务器实例管理 + Qt 信号 + 旧 `cfg.remote_*` 自动迁移 + creart 单例） |
| `src/ui/page/remote_page/connection_tester.py` | 后台 SSH 连接测试 `QRunnable` |
| `src/ui/page/remote_page/server_edit_dialog.py` | 添加 / 编辑服务器档案对话框 |
| `src/ui/page/remote_page/__init__.py` | `RemotePage` v2 重写（列表 + 详情 + 工具栏） |
| `script/test/test_local_backend.py` | LocalBackend 26 个测试 |
| `script/test/test_server_registry.py` | ServerProfile / ServerRegistry 20 个测试 |

### 1.2 扩展模块

| 路径 | 改动 |
| --- | --- |
| `src/core/remote/ssh_client.py` | 新增 5 个 SFTP 高层 op：`read_text` / `write_text` / `remote_exists` / `remote_listdir` / `remote_remove`，以及 `is_connected` 属性 |
| `src/core/remote/__init__.py` | 增加 `DeploymentState` / `ServerProfile` / `ServerRegistry` / `ServerManager` 导出 |

### 1.3 归档文件

整体迁移到 `archive/v1-remote-ui/`（语义已与 v2 不兼容）：

- `connection_base.py`（含 `mode: "ssh"\|"agent"` 双模式语义）
- `ssh_handler.py`（v1 单服务器 SSH 处理器）
- `ssh_panel.py`（全局单服务器 SSH 配置面板）
- `status_panel.py`（含 `daemon_version` 字段）
- `widgets.py`
- `tests/test_remote_page_single_path.py`（断言 `title_label == "远程 Daemon 管理"` 等 v1 概念）

详见 `archive/v1-remote-ui/README.md`。

## 2. 验收条目

### 2.1 自动化测试 — 已通过 ✅

```
python -m pytest script/test/test_local_backend.py script/test/test_server_registry.py
========================== 46 passed in 0.17s ==========================
```

新增测试覆盖：

- **文件 op**：8 个方法的正反向用例（含创建父目录、目录非递归删除拒绝、空目录列表行为）
- **检测 op**：`detect_napcat_version` 正则解析 / 文件缺失回退 / `detect_qq_path` 注册表缺失回退 / `detect_installation` 聚合
- **接口契约**：`isinstance(local, OperationBackend)`、生命周期 no-op、上下文管理器
- **延迟实现保障**：进程 / 安装写入 / 日志 / WebUI 必须 `raise NotImplementedError`，不允许默默返回错误数据
- **持久化**：序列化往返、字段缺失默认值、损坏 JSON 不阻断启动、原子写无残留 `.tmp`、跳过单条损坏档案、按 `created_at` 排序
- **安全（§6.2）**：磁盘 JSON **绝对不含** SSH 密码字面量与私钥 passphrase；`from_dict` 反序列化后密码字段为 `None`

### 2.2 手动验收（待执行）

| 步骤 | 预期 |
| --- | --- |
| 1. 启动 Desktop，导航到"远程服务器" | 显示空状态："尚未添加任何服务器" |
| 2. 点击"添加服务器" | 弹出 `ServerEditDialog`，含基本信息 / 认证 / 高级三段式表单 |
| 3. 切换"私钥 ↔ 密码" | 私钥 / 密码字段互斥显示 |
| 4. 留空主机点保存 | 表单内 `error_label` 显示"主机地址不能为空"，对话框不关闭 |
| 5. 填合法 SSH 信息保存 | 列表新增一张服务器卡片，右侧详情显示完整字段；`servers.json` 写入磁盘 |
| 6. 关闭重启 Desktop | 服务器列表保留 |
| 7. 选中服务器 → 点击"测试连接" | 后台异步测试，成功显示 `success_bar`、最近连接时间更新；失败显示 `error_bar` 含明确原因 |
| 8. 编辑服务器 → 改密码 | 内存中密码缓存更新，磁盘 JSON 不含密码 |
| 9. 删除服务器 → 二次确认 | 列表移除，`servers.json` 同步更新 |
| 10. 旧用户首次升级（曾在 setup 页配过 `cfg.remote_*`） | 启动后自动迁移成"已迁移服务器 (xxx)"档案，详情备注标注来源 |

### 2.3 安全基线（§6 复核）

- [x] 密码 / passphrase 不落盘（自动化测试覆盖）
- [x] 默认拒绝未知主机指纹（`SSHCredentials.host_key_policy="reject"`）
- [x] 默认关闭 `allow_agent` / `look_for_keys`
- [x] `private_key_path` 校验文件存在（`SSHCredentials.validate`）
- [ ] keyring 集成（计划 §10.3 排期至 **P3**）
- [ ] 首次连接指纹确认对话框（计划 §10.3 排期至 **P3**）

## 3. 接口稳定性承诺

P0 锁定下列接口签名，P1 / P2 不会破坏性修改：

- `OperationBackend` 全部 18 个抽象方法 + 4 个数据模型
- `ServerProfile` / `ServerRegistry` 公共 API
- `ServerManager` 4 个 Qt 信号：`server_added` / `server_updated` / `server_removed` / `server_state_changed`
- `servers.json` schema_version=1 结构（密码字段永远不在该 schema 中）

## 4. 进入 P1 的前置条件

- [x] `OperationBackend` 接口已冻结
- [x] `RemoteBackend` 已具备文件读写 + 状态查询能力（部署脚本上传 / 探测可基于此）
- [x] `LinuxCoreDeployment` 现有调用面零破坏（仍依赖底层 `ExecutionBackend`）
- [x] 多服务器档案与状态机（`DeploymentState`）就绪，可承接部署进度

## 5. 已知 pre-existing 测试失败（与 P0 无关）

以下 16 个失败 / 10 个 collection error 在 P0 修改之前即已存在（`component_page` 模块缺失、`download_url` KeyError 等），与本次工作无交集，**不构成 P0 阻塞**：

```
component_page.base 模块不存在 → 10 collection errors
test_downloader.py（10 个）/ test_email.py / test_get_version.py / 等 → 16 failed
```

P0 范围内：**46 / 46 通过**，无新增失败。
