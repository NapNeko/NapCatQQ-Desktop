# -*- coding: utf-8 -*-
"""
## 安装逻辑
"""
# 标准库导入
import hashlib
import shutil
import subprocess
import zipfile
from pathlib import Path
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from PySide6.QtCore import QObject, QRunnable, Signal

# 项目内模块导入
from src.core.common.status import ButtonStatus, ProgressRingStatus
from src.core.installation.errors import NapCatHashMismatchError
from src.core.logging import LogSource, LogType, logger
from src.core.runtime.paths import PathFunc

if TYPE_CHECKING:
    from src.core.versioning.release_hash_service import ReleaseHashService


# 流式读取 chunk 大小; 4MB 在内存与系统调用次数之间取折中
_HASH_CHUNK_SIZE = 4 * 1024 * 1024


def verify_napcat_archive(
    *,
    version: str,
    archive_path: Path,
    hash_service: "ReleaseHashService",
) -> bool:
    """对 NapCat 安装包做 SHA256 完整性校验 (P5 安全收尾 F1.3).

    Args:
        version: 期望版本号, 接受 ``"v4.18.2"`` / ``"4.18.2"`` 两种形式
        archive_path: 待校验的本地 zip 路径
        hash_service: 已经 ``fetch()`` 过的 :class:`ReleaseHashService`

    Returns:
        - ``True``: 校验通过, archive 内容与上游 hash 一致, 调用方可继续解压
        - ``False``: 上游 release 中没有该版本的 hash 数据 (网络异常无缓存 /
          GitHub Releases API 仅返回 latest 而本地版本与之不匹配). 调用方应弹
          "二次确认"对话框让用户决定是否继续. archive 不会被删除.

    Raises:
        NapCatHashMismatchError: archive 的实际 SHA256 与上游期望不一致;
            archive 文件**已被删除**, 防止后续误用.
        FileNotFoundError: archive 路径不存在.
    """
    if not archive_path.exists():
        raise FileNotFoundError(f"待校验的 archive 不存在: {archive_path}")

    entry = hash_service.lookup(version)
    if entry is None:
        logger.warning(
            (
                f"verify_napcat_archive: 上游未提供 {version} 的 SHA256, 跳过校验; "
                "调用方应在 UI 层弹二次确认对话框"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        return False

    expected = entry.shell_sha256
    actual = _compute_sha256(archive_path)
    if actual.lower() != expected.lower():
        logger.error(
            (
                "verify_napcat_archive: SHA256 不匹配, 已拒绝安装并删除 archive: "
                f"version={entry.version}, expected={expected}, actual={actual}, "
                f"archive={archive_path}"
            ),
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        try:
            archive_path.unlink()
        except OSError as exc:
            logger.warning(
                f"verify_napcat_archive: 删除非法 archive 失败 (忽略): {exc!r}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
        raise NapCatHashMismatchError(
            version=entry.version,
            expected=expected,
            actual=actual,
            archive_path=str(archive_path),
        )

    logger.info(
        f"verify_napcat_archive: SHA256 校验通过 (version={entry.version}, archive={archive_path})",
        LogType.FILE_FUNC,
        LogSource.CORE,
    )
    return True


def _compute_sha256(path: Path) -> str:
    """流式计算 ``path`` 的 SHA256 hex digest."""
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(_HASH_CHUNK_SIZE)
            if not chunk:
                break
            hasher.update(chunk)
    return hasher.hexdigest()


class InstallBase(QObject, QRunnable):
    """安装工具基类, 包含通用信号"""

    # 安装成功信号
    install_finish_signal = Signal()
    # 安装失败信号
    error_finish_signal = Signal()
    # 按钮模式切换
    button_toggle_signal = Signal(ButtonStatus)
    # 进度条模式切换
    progress_ring_toggle_signal = Signal(ProgressRingStatus)
    # 状态标签
    status_label_signal = Signal(str)

    def __init__(self) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)

    def run(self) -> None:
        """运行安装逻辑"""
        self.execute()

    def execute(self) -> None:
        """执行安装逻辑 (子类必须实现)"""
        raise NotImplementedError("Subclasses must implement this method")


class NapCatInstall(InstallBase):
    """NapCat 安装逻辑"""

    def __init__(self) -> None:
        super().__init__()
        self.zip_file_path = it(PathFunc).tmp_path / "NapCat.Shell.zip"
        self.install_path = it(PathFunc).napcat_path

    def execute(self) -> None:
        """安装逻辑"""
        try:
            logger.info(f"开始安装 NapCat: target={self.install_path}", LogType.FILE_FUNC, LogSource.CORE)
            self.status_label_signal.emit("正在安装 NapCat")
            self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)
            self.ensure_install_path()
            # 移除 NapCat 文件夹下除了 config 和 log 文件夹外的所有文件
            self.remove_old_file()
            # 解压文件
            self.unzip_file()

        except Exception as e:
            self.status_label_signal.emit(self.tr("安装失败"))
            self.error_finish_signal.emit()
            logger.exception("安装 NapCat 失败", e, LogType.FILE_FUNC, LogSource.CORE)

    def ensure_install_path(self) -> None:
        """确保安装目录存在. """
        if self.install_path.exists():
            return

        logger.warning(
            f"NapCat 安装目录不存在，准备重新创建: target={self.install_path}",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        self.install_path.mkdir(parents=True, exist_ok=True)

    def remove_old_file(self) -> None:
        """删除旧文件"""
        logger.info(f"开始删除 NapCat 旧文件: target={self.install_path}", LogType.FILE_FUNC, LogSource.CORE)
        self.status_label_signal.emit("正在删除旧文件")
        self.ensure_install_path()

        for item in self.install_path.iterdir():
            if item.is_dir() and item.name not in ["config", "log"]:
                shutil.rmtree(item)
            elif item.is_file():
                item.unlink()
        self.status_label_signal.emit("旧文件删除成功")
        logger.info("NapCat 旧文件删除完成", LogType.FILE_FUNC, LogSource.CORE)

    def unzip_file(self) -> None:
        """解压文件"""
        logger.info(
            f"开始解压 NapCat 安装包: source={self.zip_file_path}, target={self.install_path}",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        self.status_label_signal.emit("正在解压文件")
        self.ensure_install_path()
        with zipfile.ZipFile(self.zip_file_path, "r") as zip_ref:
            zip_ref.extractall(self.install_path)
        self.zip_file_path.unlink()  # 移除安装包
        self.install_finish_signal.emit()
        logger.info("NapCat 安装完成", LogType.FILE_FUNC, LogSource.CORE)


class SnowLumaInstall(InstallBase):
    """SnowLuma 发布包解压安装器 (P1 SnowLuma 适配).

    与 :class:`NapCatInstall` 同文件, 同信号名, 让上层可以走 NapCatPage 同款
    连接模式. 关键区别:

    - zip 文件名含版本号 (`SnowLuma-<tag>-win-x64.zip`), 与 NapCat 固定 `NapCat.Shell.zip` 不同
    - 解压时**保留** ``config/`` 与 ``data/`` 子目录中的现有文件 (避免覆盖用户运行期修改)
    - zip 内顶层目录 ``SnowLuma-<tag>-win-x64/`` 需被剥离, 实际文件应落在 install_path 根下
    - 安装成功后写 ``.installed_tag``, 供 :class:`LocalVersionTask.get_snowluma_version` 读取
    """

    def __init__(self, tag: str) -> None:
        super().__init__()
        if not isinstance(tag, str) or not tag.strip():
            raise ValueError("tag 不可为空; 需传入 release tag (含 v 前缀, 如 'v1.7.5')")
        self._tag = tag.strip()
        self.zip_file_path = it(PathFunc).tmp_path / f"SnowLuma-{self._tag}-win-x64.zip"
        self.install_path = it(PathFunc).snowluma_path

    def execute(self) -> None:
        """安装逻辑"""
        try:
            logger.info(
                f"开始安装 SnowLuma: tag={self._tag}, target={self.install_path}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
            self.status_label_signal.emit("正在安装 SnowLuma")
            self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)
            self.ensure_install_path()
            self.unzip_file()
            self.verify_install()
            self.write_installed_tag()
            # P2 (Tier G): 安装完成后立即生成 / 同步 Desktop 主导的 SnowLuma WebUI 密码;
            # 让用户感知不到 WebUI 密码即可一键启动 Bot.
            self._init_or_update_password()
            self.install_finish_signal.emit()
            logger.info(
                f"SnowLuma 安装完成: tag={self._tag}", LogType.FILE_FUNC, LogSource.CORE
            )
        except Exception as e:
            self.status_label_signal.emit(self.tr("安装失败"))
            self.error_finish_signal.emit()
            logger.exception("安装 SnowLuma 失败", e, LogType.FILE_FUNC, LogSource.CORE)
            # 异常路径上清理临时 zip
            try:
                self.zip_file_path.unlink(missing_ok=True)
            except OSError:
                pass

    def ensure_install_path(self) -> None:
        """确保安装目录存在. """
        if not self.install_path.exists():
            logger.warning(
                f"SnowLuma 安装目录不存在，准备创建: target={self.install_path}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
        self.install_path.mkdir(parents=True, exist_ok=True)

    def unzip_file(self) -> None:
        """解压 SnowLuma 发布包 (保留 config/ 与 data/ 已有文件).

        上游 GitHub release 的 ``SnowLuma-<tag>-win-x64.zip`` 实际是**扁平结构**
        (顶层直接是 ``client/``, ``native/``, ``index.mjs`` 等), 没有
        ``SnowLuma-<tag>-win-x64/`` 包装目录. 但若用户手工/三方工具重打包后
        zip 含有单一包装目录, 我们也应正确处理.

        因此本方法在解压前先自动探测 zip 是否带单一包装目录:
        - 当所有条目共享同一顶层目录 (且不存在顶层文件) 时, 认为存在包装,
          解压时剥离该前缀
        - 否则视为扁平 zip, 按原路径解压
        """
        logger.info(
            f"开始解压 SnowLuma 发布包: source={self.zip_file_path}, target={self.install_path}",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        self.status_label_signal.emit("正在解压文件")
        self.ensure_install_path()

        with zipfile.ZipFile(self.zip_file_path, "r") as zf:
            members = zf.namelist()
            strip_prefix = self._detect_wrapper_prefix(members)
            if strip_prefix is not None:
                logger.info(
                    f"探测到 SnowLuma zip 带包装目录, 解压时剥离: {strip_prefix!r}",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
            else:
                logger.info(
                    "SnowLuma zip 为扁平结构 (无包装目录), 按原路径解压",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )

            for member in members:
                if strip_prefix is not None and member.startswith(strip_prefix):
                    stripped = member[len(strip_prefix):]
                else:
                    stripped = member
                if not stripped:
                    # 包装目录自身条目 (如 "SnowLuma-v1.7.5-win-x64/"), 跳过
                    continue
                out = self.install_path / stripped
                if member.endswith("/"):
                    out.mkdir(parents=True, exist_ok=True)
                    continue
                if stripped.startswith(("config/", "data/")) and out.exists():
                    # 保留用户运行时修改
                    continue
                out.parent.mkdir(parents=True, exist_ok=True)
                with zf.open(member) as src, out.open("wb") as dst:
                    shutil.copyfileobj(src, dst)

        self.zip_file_path.unlink()  # 移除安装包
        logger.info("SnowLuma 解压完成", LogType.FILE_FUNC, LogSource.CORE)

    @staticmethod
    def _detect_wrapper_prefix(members: list[str]) -> str | None:
        """探测 zip 是否带单一顶层包装目录, 是则返回需剥离的前缀 (含尾 ``/``).

        判定规则: 所有非空条目的首段相同, 且不存在仅有首段而无内层文件的退化情况.
        例如:

        - ``["SnowLuma-v1.7.5-win-x64/", "SnowLuma-v1.7.5-win-x64/index.mjs", ...]``
          → 返回 ``"SnowLuma-v1.7.5-win-x64/"``
        - ``["client/", "client/index.html", "native/", "index.mjs", ...]`` (扁平)
          → 返回 ``None``
        """
        first_segments: set[str] = set()
        has_inner_entry = False
        for raw in members:
            cleaned = raw.rstrip("/")
            if not cleaned:
                continue
            if "/" in cleaned:
                first_segments.add(cleaned.split("/", 1)[0])
                has_inner_entry = True
            else:
                first_segments.add(cleaned)
        if len(first_segments) != 1 or not has_inner_entry:
            return None
        return next(iter(first_segments)) + "/"

    def verify_install(self) -> None:
        """验证关键产物存在. """
        for required in ("node.exe", "index.mjs", "package.json"):
            if not (self.install_path / required).exists():
                raise RuntimeError(f"SnowLuma 安装产物缺失: {required}")

    def write_installed_tag(self) -> None:
        """记录已安装的 release tag (供 LocalVersionTask 读)."""
        target = self.install_path / ".installed_tag"
        target.write_text(self._tag, encoding="utf-8")

    def _init_or_update_password(self) -> None:
        """安装成功后初始化 / 维持 Desktop 主导的 SnowLuma WebUI 密码 (P2 Tier G).

        幂等行为:

        - 如果 ``snowluma-session.json`` 已存在 + 字段完整: sticky 不改密码值,
          仅同步 ``render_webui_json`` 让 SnowLuma 侧 ``webui.json`` 与 Desktop session.json
          一致 (覆盖用户在 WebUI 改过的密码; D2 决策);
        - 如果不存在 / 损坏: 生成新强密码, 写 session.json, 同步 webui.json.

        本方法**不抛**任何异常: 安装本身不应被密码生成失败阻塞 (用户仍可手动用 SnowLuma
        WebUI 自治流程登录). 失败时 logger.warning 让用户感知.
        """
        # 延迟导入避免在 installer 类加载阶段拖入 QObject 依赖;
        # 也避免在不需要 SnowLuma 的环境下污染创建期路径.
        from src.core.runtime.snowluma_config_renderer import render_webui_json
        from src.core.runtime.snowluma_session import (
            create_session,
            load_session,
            update_last_rendered,
        )

        try:
            session = load_session()
            if session is None:
                session = create_session()
                logger.info(
                    "已生成新的 SnowLuma WebUI 密码 (sticky)",
                    LogType.FILE_FUNC,
                    LogSource.CORE,
                )
            # 强制把 Desktop 侧密码同步到 SnowLuma webui.json (单向覆盖, D2).
            render_webui_json(self.install_path, password=session.password, must_change=False)
            update_last_rendered(session)
        except Exception as exc:  # noqa: BLE001 - 不让密码失败阻塞安装主流程
            logger.warning(
                f"SnowLuma WebUI 密码初始化失败 (非致命): {type(exc).__name__}: {exc}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )


class QQInstall(InstallBase):
    """QQ 安装逻辑"""

    def __init__(self, exe_path: str | Path) -> None:
        super().__init__()
        self.exe_path: Path = exe_path if isinstance(exe_path, Path) else Path(exe_path)

    def execute(self) -> None:
        """安装逻辑"""
        try:
            logger.info(f"开始安装 QQ: installer={self.exe_path}", LogType.FILE_FUNC, LogSource.CORE)
            self.status_label_signal.emit("正在安装 QQ")
            self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)

            # 启动 QQ 安装程序
            result = subprocess.run([str(self.exe_path), "/s"])
            if result.returncode == 0:
                self.install_finish_signal.emit()
                logger.info("QQ 安装完成", LogType.FILE_FUNC, LogSource.CORE)
            else:
                self.error_finish_signal.emit()
                logger.error(f"QQ 安装程序返回非零退出码: {result.returncode}", LogType.FILE_FUNC, LogSource.CORE)

            self.exe_path.unlink()  # 移除安装包

        except Exception as e:
            self.status_label_signal.emit(self.tr(f"安装失败: {e}"))
            self.error_finish_signal.emit()
            logger.exception("安装 QQ 失败", e, LogType.FILE_FUNC, LogSource.CORE)

