# -*- coding: utf-8 -*-
"""
此模块用于处理 NCD 中的版本获取问题
"""

# 标准库导入
import json
import re
from collections.abc import Callable

# 第三方库导入
from creart import it
import httpx
from pydantic import BaseModel, ValidationError
from PySide6.QtCore import QObject, QRunnable, QThreadPool, QUrl, Signal

# 项目内模块导入
from src.core.config import cfg
from src.core.network.urls import Urls
from src.core.utils.logger import logger
from src.core.utils.path_func import PathFunc


class DesktopUpdateMigration(BaseModel):
    """Desktop 版本区间迁移规则。"""

    id: str
    from_min: str | None = None
    from_max: str | None = None
    to_version: str | None = None
    to_min: str | None = None
    to_max: str | None = None
    strategy: str = "remote_script"
    script_url: str | None = None
    summary: str | None = None

    def matches(self, local_version: str | None, remote_version: str | None) -> bool:
        """判断当前规则是否命中本地与目标版本组合。"""

        if not local_version or not remote_version:
            return False

        if self.strategy == "remote_script" and not self.script_url:
            return False

        if not _version_in_range(local_version, self.from_min, self.from_max):
            return False

        if self.to_version:
            return _compare_versions(remote_version, self.to_version) == 0

        return _version_in_range(remote_version, self.to_min, self.to_max)


class DesktopUpdateManifest(BaseModel):
    """Desktop 远端升级策略清单。"""

    schema_version: int = 2
    min_auto_update_version: str | None = None
    migrations: list[DesktopUpdateMigration] = []


class DesktopUpdatePlan(BaseModel):
    """Desktop 更新决策结果。"""

    kind: str
    summary: str | None = None
    min_auto_update_version: str | None = None
    migration: DesktopUpdateMigration | None = None

    def requires_remote_script(self) -> bool:
        """当前计划是否需要远端迁移脚本。"""

        return bool(
            self.kind == "migration"
            and self.migration is not None
            and self.migration.strategy == "remote_script"
            and self.migration.script_url
        )

    def blocks_update(self) -> bool:
        """当前计划是否阻止自动更新。"""

        return self.kind == "unsupported"


class VersionData(BaseModel):
    """版本信息数据模型"""

    # 版本号
    napcat_version: str | None
    qq_version: str | None
    ncd_version: str | None

    # 下载链接
    qq_download_url: str | None = None

    # 更新日志
    napcat_update_log: str | None = None
    ncd_update_log: str | None = None
    ncd_update_manifest: DesktopUpdateManifest | None = None


class VersionRunnableBase(QObject, QRunnable):
    """版本获取基类"""

    version_signal = Signal(VersionData)
    finish_signal = Signal()
    error_signal = Signal(str)

    def __init__(self) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)

    def run(self) -> None:
        """运行任务"""
        self.version_signal.emit(self.execute())

    def execute(self) -> VersionData:
        """子类实现此方法以执行任务"""
        raise NotImplementedError("Subclasses must implement this method")


class GetRemoteVersionRunnable(VersionRunnableBase):
    """获取远程版本信息

    运行流程:
    1. 请求 NapCat 版本信息
    2. 请求 QQ 版本信息
    3. 请求 NapCatQQ Desktop 版本信息
    4. 返回 VersionData 实例
    5. 通过 version_signal 发射版本信息
    6. 通过 finish_signal 发射任务完成信号
    7. 如果发生错误, 通过 error_signal 发射错误信息

    """

    def __init__(self) -> None:
        super().__init__()

    def execute(self) -> VersionData:
        """执行获取远程版本信息的任务"""
        napcat_info = self._get_version(Urls.NAPCATQQ_REPO_API.value, "NapCat", self._parse_github_response)
        qq_version = self._get_version(Urls.QQ_Version.value, "QQ", self._parse_qq_response)
        ncd_version = self._get_version(Urls.NCD_REPO_API.value, "NapCatQQ Desktop", self._parse_github_response)
        ncd_update_manifest = self._get_desktop_update_manifest()

        return VersionData(
            napcat_version=napcat_info["version"],
            qq_version=qq_version["version"],
            ncd_version=ncd_version["version"],
            qq_download_url=qq_version["download_url"],
            napcat_update_log=napcat_info["update_log"],
            ncd_update_log=ncd_version["update_log"],
            ncd_update_manifest=ncd_update_manifest,
        )

    def _get_version(
        self, url: str | QUrl, name: str, parser: Callable[[dict], dict[str, str | None]]
    ) -> dict[str, str | None]:
        """获取指定服务的版本信息

        Args:
            url (str | QUrl): 服务的 API URL
            name (str): 服务名称
            parser (Callable): 用于解析响应的函数

        Returns:
            dict: 包含版本信息的字典
        """
        response = self.request(QUrl(url), name)

        if response is None:
            return self._get_error_value(name)

        try:
            return parser(response)
        except KeyError as e:
            logger.error(f"解析 {name} 版本信息失败: {e}")
            self.error_signal.emit(f"解析 {name} 版本信息失败: {e}")
            return self._get_error_value(name)

    def _get_error_value(self, name: str) -> dict[str, str | None]:
        """根据服务名称返回对应的错误值"""
        error_values: dict[str, dict[str, str | None]] = {
            "QQ": {"version": None, "download_url": None},
            "NapCat": {"version": None, "update_log": None},
            "NapCatQQ Desktop": {"version": None, "update_log": None},
        }
        return error_values.get(name, {"version": None})

    def _parse_github_response(self, response: dict) -> dict[str, str | None]:
        """解析 GitHub API 响应格式"""
        return {"version": response["tag_name"], "update_log": response["body"]}

    def _parse_qq_response(self, response: dict) -> dict[str, str | None]:
        """解析 QQ 的新接口返回格式"""
        if not response:
            return {"version": None, "download_url": None}

        try:
            result = response.get("Windows")
            if result is not None:
                return {"version": result.get("version", ""), "download_url": result.get("ntDownloadX64Url")}
            else:
                return {"version": None, "download_url": None}
        except Exception as e:
            logger.error(f"解析 QQ 版本信息失败: {e}")
            self.error_signal.emit(f"解析 QQ 版本信息失败: {e}")
            return {"version": None, "download_url": None}

    def _get_desktop_update_manifest(self) -> DesktopUpdateManifest | None:
        """获取 Desktop 的远端升级策略清单。"""

        response = self.request(Urls.NCD_UPDATE_MANIFEST.value, "NapCatQQ Desktop 更新策略", use_mirrors=True)
        if response is None:
            return None

        try:
            return DesktopUpdateManifest.model_validate(response)
        except ValidationError as exc:
            logger.error(f"解析 NapCatQQ Desktop 更新策略失败: {exc}")
            self.error_signal.emit(f"解析 NapCatQQ Desktop 更新策略失败: {exc}")
            return None

    def request(self, url: QUrl, name: str, use_mirrors: bool = False) -> dict[str, str] | None:
        """网络请求"""
        request_urls = [url.url()]
        if use_mirrors:
            request_urls.extend(f"{mirror.toString().rstrip('/')}/{url.url()}" for mirror in Urls.MIRROR_SITE.value)

        last_error: Exception | None = None
        for candidate_url in request_urls:
            try:
                with httpx.Client(timeout=5, follow_redirects=True) as client:
                    response = client.get(candidate_url)
                    response.raise_for_status()
                    return response.json()
            except (httpx.RequestError, httpx.HTTPStatusError, ValueError) as exc:
                last_error = exc

        logger.error(f"获取 {name} 版本信息失败: {last_error}")
        self.error_signal.emit(f"获取 {name} 版本信息失败: {last_error}")
        return None


class GetLocalVersionRunnable(VersionRunnableBase):
    """获取本地版本信息

    运行流程:
    1. 读取本地 NapCat 版本信息
    2. 读取本地 QQ 版本信息
    3. 读取本地 NapCatQQ Desktop 版本信息
    4. 返回 VersionData 实例
    5. 通过 version_signal 发射版本信息
    6. 通过 finish_signal 发射任务完成信号
    7. 如果发生错误, 通过 error_signal 发射错误信息

    """

    def __init__(self) -> None:
        super().__init__()

    def execute(self) -> VersionData:
        """执行获取本地版本信息的任务"""

        return VersionData(
            napcat_version=self.get_napcat_version(),
            qq_version=self.get_qq_version(),
            ncd_version=self.get_ncd_version(),
        )

    def get_napcat_version(self) -> str | None:
        """获取本地 NapCat 版本信息"""
        napcat_path = it(PathFunc).napcat_path

        if version := self._get_napcat_version_from_mjs(napcat_path / "napcat.mjs"):
            return version

        try:
            with open(str(napcat_path / "package.json"), "r", encoding="utf-8") as f:
                # 读取到参数返回版本信息
                return f"v{json.loads(f.read())['version']}"
        except FileNotFoundError:
            logger.error("获取 NapCat 版本信息失败: 文件不存在")
            self.error_signal.emit("获取 NapCat 版本信息失败: 文件不存在")
            return None

    @staticmethod
    def _get_napcat_version_from_mjs(mjs_path) -> str | None:
        """从 napcat.mjs 中提取构建时内联的 NapCat 核心版本。"""
        try:
            content = mjs_path.read_text(encoding="utf-8")
        except FileNotFoundError:
            return None

        match = re.search(r'napCatVersion\s*=\s*.*?"(\d+\.\d+\.\d+(?:[-+][^"]+)?)"', content)
        if match is None:
            return None

        return f"v{match.group(1)}"

    def get_qq_version(self) -> str | None:
        """获取本地 QQ 版本信息"""
        try:
            if (qq_path := it(PathFunc).get_qq_path()) is None:
                # 检查 QQ 目录是否存在
                logger.error("获取 QQ 版本信息失败: 文件不存在")
                return None

            with open(str(qq_path / "versions" / "config.json"), "r", encoding="utf-8") as file:
                # 读取 config.json 文件获取版本信息 数据为:9.9.23-41857 只返回'9.9.23'
                return json.load(file)["curVersion"].split("-")[0]
        except FileNotFoundError:
            # 文件不存在则返回 None
            logger.error("获取 QQ 版本信息失败: 文件不存在")
            self.error_signal.emit("获取 QQ 版本信息失败: 文件不存在")
            return None

    def get_ncd_version(self) -> str | None:
        """获取本地 NapCatQQ Desktop 版本信息"""
        return cfg.get(cfg.napcat_desktop_version)


class GetVersion(QObject):

    # 获取版本结束信号
    remote_finish_signal = Signal(VersionData)
    local_finish_signal = Signal(VersionData)

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)

    def update(self) -> None:
        """开始更新版本信息"""
        # 本地版本检查
        local_runnable = GetLocalVersionRunnable()
        local_runnable.version_signal.connect(self.local_finish_signal.emit)
        QThreadPool.globalInstance().start(local_runnable)

        # 远程版本检查
        remote_runnable = GetRemoteVersionRunnable()
        remote_runnable.version_signal.connect(self.remote_finish_signal.emit)
        QThreadPool.globalInstance().start(remote_runnable)


def resolve_desktop_update_plan(
    local_version: str | None,
    remote_version: str | None,
    manifest: DesktopUpdateManifest | None,
) -> DesktopUpdatePlan | None:
    """根据本地版本、目标版本和 manifest 解析 Desktop 更新策略。"""

    if not local_version or not remote_version:
        return None

    if _compare_versions(local_version, remote_version) >= 0:
        return None

    if manifest is None:
        return None

    if manifest.min_auto_update_version and _compare_versions(local_version, manifest.min_auto_update_version) < 0:
        return DesktopUpdatePlan(
            kind="unsupported",
            summary="当前版本过旧，已超出自动升级支持范围。",
            min_auto_update_version=manifest.min_auto_update_version,
        )

    for migration in manifest.migrations:
        if migration.matches(local_version, remote_version):
            return DesktopUpdatePlan(kind="migration", summary=migration.summary, migration=migration)

    return None


def _compare_versions(left: str, right: str) -> int:
    """比较两个版本号。"""

    left_parts = _normalize_version(left)
    right_parts = _normalize_version(right)

    for left_part, right_part in zip(left_parts, right_parts):
        if left_part < right_part:
            return -1
        if left_part > right_part:
            return 1

    return 0


def _normalize_version(version: str) -> tuple[int, ...]:
    """将 v1.2.3 这类版本号归一化为整数元组。"""

    match = re.search(r"(\d+(?:\.\d+)*)", version)
    if match is None:
        return (0,)

    parts = tuple(int(part) for part in match.group(1).split("."))
    if len(parts) >= 3:
        return parts

    return parts + (0,) * (3 - len(parts))


def _version_in_range(version: str, min_version: str | None, max_version: str | None) -> bool:
    """判断版本是否处于闭区间范围内。"""

    if min_version and _compare_versions(version, min_version) < 0:
        return False

    if max_version and _compare_versions(version, max_version) > 0:
        return False

    return True
