## 快速上手 — 给 AI 协助编码器的说明

下面是为能够让 AI 代码助手（Copilot / 其他 agent）快速投入该仓库而写的简明指导。目标是：立即定位关键组件、理解项目约定、并能安全地修改与打包。请保持简洁、使用项目中已有模式和文件路径作为示例。

### 1) 项目概览（大致架构）
- 入口：`main.py`（桌面应用入口）。
- UI：全部位于 `src/ui/`，使用 PySide6（基于 Signals/Slots + `QObject`）。
- 核心逻辑：`src/core/` 下分模块：`config`（配置模型），`network`（http/email/webhook 等），`utils`（PathFunc、logger、creart 注入等）。
- 资源与运行时：`runtime/`（包含 `NapCatQQ/` 的二进制与脚本、`config/` 的运行时 json）。
- 打包/发布：仓库内含 PyInstaller 相关依赖与 `.spec`（`script/build_scripts/*.spec`），`pyproject.toml` 中声明了 `pyinstaller` 依赖。

为什么这样组织：UI 与业务逻辑被分层（`ui` vs `core`），外部 NapCat 子进程与插件放在 `runtime/NapCatQQ`，而 `src/core/utils/run_napcat.py` 管理 QProcess 与进程日志，是与 NapCat 本体交互的关键点。

### 2) 关键文件/位置（立即查看）
- `pyproject.toml` — Python 版本（3.12）和主要依赖（PySide6-Fluent-Widgets、creart、psutil、pyinstaller 等）。
- `main.py` — 应用入口，启动 UI/事件循环。
- `src/core/utils/run_napcat.py` — NapCat 进程管理（创建 QProcess、设置环境变量、写入 loadNapCat.js、收集日志、kill 子进程）。修改 NapCat 启动逻辑或环境变量，优先修改此文件。
- `src/core/config/config_model.py` — 配置/数据模型（很多模块通过 Config 传递设置）。
- `src/core/utils/path_func.py` — 路径封装，所有对 `runtime/NapCatQQ` 的路径引用都通过它。
- `src/core/utils/logger.py` — 日志封装；在修改全局日志行为前先查看此文件。
- `runtime/config/*.json` — 运行时 bot 配置示例（QQID、onebot 等），调试时可直接替换这些 json。

### 3) 常见代码模式与约定（从仓库可观察到的）
- 依赖注入 / 工厂：项目使用 `creart`（见 `add_creator`, `exists_module`, `it()`）。创建单例/管理器时使用 `AbstractCreator` 子类并在模块末尾 `add_creator` 注册。
  - 例：`ManagerNapCatQQProcessCreator` 在 `run_napcat.py` 中声明 targets 并返回 `create_type()`。
- PySide6 风格：UI 组件继承 `QObject`，自定义 Signal，使用 `process.readyReadStandardOutput.connect(handler)` 等连接模式。
- 子进程：NapCat 运行以 `QProcess` 启动，必须设置 `setEnvironment()` 与 `setProgram()`/`setArguments()`。当终止时使用 `psutil` 拆解子进程树并 kill（参考 `stop_process` 实现）。
- 路径、资源处理：通过 `PathFunc()` 获取 napcat 路径、QQ.exe 路径等，避免硬编码路径。
- 格式化与导入顺序：项目配置了 `black`、`isort`（查看 `pyproject.toml`），请遵循现有格式化设置（line-length=120，PySide6 单独分组）。

### 4) 开发 / 运行 / 打包 工作流（可直接使用的命令与提示）
- 虚拟环境：仓库常见使用 `.venv`（例子：`.venv\Scripts\python.exe main.py`）。在 Windows PowerShell 下运行：
  ```powershell
  # 激活或直接用解释器运行
  .\.venv\Scripts\Activate.ps1
  python main.py
  ```
- 调试：在 VSCode 中调试会用到 `debugpy`（见会话历史中的 debugpy 启动片段）。直接用 VSCode 的 Python debug 配置启动 `main.py` 即可。
- 打包（PyInstaller）：仓库包含 `.spec`（`script/build_scripts/*.spec`），使用 `pyinstaller` 打包前先确保项目依赖安装在运行时环境（python 3.12）。示例：
  ```powershell
  .\.venv\Scripts\python.exe -m PyInstaller -y script\build_scripts\main.spec
  ```
- 运行时配置：运行程序需关注 `runtime/config/*.json`（bot 配置），以及 `runtime/NapCatQQ/` 下的二进制/脚本是否存在。

### 5) 编辑/修改时的安全区与注意点（具体到本仓库）
- 修改 NapCat 启动/注入逻辑：优先修改 `src/core/utils/run_napcat.py`，并同时检查 `PathFunc()` 的实现以保证路径生成正确。
- 添加新 Creator：按照 `creart` 模式（参考现有 Creator），将 `targets` 指向 `src.<模块路径>` 与类名，`available()` 应检查模块存在性。
- 修改 UI 信号/槽：保持 QObject 信号签名一致，使用现有 `Signal(...)` 定义；不要直接在非 UI 线程操作 UI 对象。
- 子进程终止：必须使用 `psutil` 清理子进程树（不要只 kill 父进程），否则会留下僵尸进程；参考 `stop_process` 的实现。

### 6) 典型例子（便于复制的 checklist）
 - 想要读取某个 QQ 的 NapCat 日志：打开 `it(NapCatQQLogManager).get_log(qq_id)`，或查看 `NapCatQQProcessLog.get_log_content()`。
 - 新增配置字段：先修改 `src/core/config/config_model.py` 的 Pydantic/数据类，再同步 UI 表单与 `runtime/config/*.json`。

### 7) 小贴士（避免踩坑）
- Python 版本严格（pyproject 指定 `==3.12.*`），在其他 Python 版本运行可能出现不兼容依赖或语法。
- 多处使用 `Path.as_uri()` 与 `QProcess` 环境变量拼接，注意 Windows 路径转义与 URI 表示法。
- 查找单例/工厂：搜索 `add_creator(`、`it(`、`exists_module(`。

---
如果有特别想补充的约定（例如某些目录是手工维护、某些脚本依赖外部账号），请指出我会把它加入并迭代此文件。你希望我现在把它提交到仓库并运行一次简要检查（例如格式/存在性校验）吗？
