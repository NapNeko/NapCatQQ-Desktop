# -*- coding: utf-8 -*-
# 标准库导入
from pathlib import Path
from threading import Event, Lock
from typing import ClassVar

# 第三方库导入
import httpx
from creart import it
from PySide6.QtCore import QObject, QRunnable, QUrl, Signal

# 项目内模块导入
from src.core.common.status import ButtonStatus, ProgressRingStatus
from src.core.network.urls import Urls
from src.core.logging import LogSource, LogType, logger
from src.core.logging.crash_bundle import summarize_url, summarize_path
from src.core.runtime.paths import PathFunc


class DownloaderBase(QObject, QRunnable):
    """下载器基类

    定义了3个信号:
    - download_progress_signal(int): 下载进度, 范围 0-100
    - download_finish_signal(): 下载完成
    - error_finsh_signal(): 引发错误导致结束
    """

    # 下载进度
    download_progress_signal = Signal(int)
    # 下载完成
    download_finish_signal = Signal()
    # 下载暂停
    download_paused_signal = Signal()
    # 下载取消
    download_canceled_signal = Signal()
    # 引发错误导致结束
    error_finsh_signal = Signal()

    # 按钮模式切换
    button_toggle_signal = Signal(ButtonStatus)
    # 进度条模式切换
    progress_ring_toggle_signal = Signal(ProgressRingStatus)
    # 状态标签
    status_label_signal = Signal(str)

    _active_target_lock: ClassVar[Lock] = Lock()
    _active_target_paths: ClassVar[set[str]] = set()

    def __init__(self) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self._pause_requested = Event()
        self._cancel_requested = Event()

    @staticmethod
    def _target_key(target_path: Path) -> str:
        """将目标路径规范化为可用于并发保护的键。"""
        return str(target_path.resolve(strict=False))

    def try_acquire_target(self, target_path: Path) -> bool:
        """尝试为当前下载目标获取独占锁，避免重复写入同一文件。"""
        target_key = self._target_key(target_path)
        with self._active_target_lock:
            if target_key in self._active_target_paths:
                self.status_label_signal.emit(self.tr("当前下载任务仍在进行，请稍候"))
                logger.warning(
                    f"检测到重复下载请求，已忽略: target={summarize_path(target_path)}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                self.error_finsh_signal.emit()
                return False

            self._active_target_paths.add(target_key)
        return True

    @classmethod
    def release_target(cls, target_path: Path) -> None:
        """释放下载目标锁。"""
        target_key = cls._target_key(target_path)
        with cls._active_target_lock:
            cls._active_target_paths.discard(target_key)

    @staticmethod
    def safe_unlink(path: Path) -> bool:
        """安全删除文件，避免清理失败导致线程崩溃。"""
        if not path.exists():
            return True

        try:
            path.unlink(missing_ok=True)
            return True
        except PermissionError as exc:
            logger.warning(
                f"清理下载临时文件失败(文件被占用): path={summarize_path(path)}, error={exc}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )
        except OSError as exc:
            logger.warning(
                f"清理下载临时文件失败: path={summarize_path(path)}, error={exc}",
                LogType.FILE_FUNC,
                LogSource.CORE,
            )

        return False

    def request_pause(self) -> None:
        """请求暂停当前下载。"""
        self._pause_requested.set()

    def request_cancel(self) -> None:
        """请求取消当前下载。"""
        self._cancel_requested.set()

    def _ensure_not_interrupted(self, partial_path: Path) -> None:
        """在下载循环中检查暂停/取消请求。"""
        if self._cancel_requested.is_set():
            raise DownloadCanceledError

        if self._pause_requested.is_set():
            raise DownloadPausedError

    @staticmethod
    def _resolve_total_size(response, existing_size: int) -> int:
        """解析下载总大小，兼容 Range 续传。"""
        content_length = int(response.headers.get("content-length", 0) or 0)
        if getattr(response, "status_code", 200) == 206:
            content_range = response.headers.get("content-range", "")
            if "/" in content_range:
                total_text = content_range.rsplit("/", 1)[-1]
                if total_text.isdigit():
                    return int(total_text)
            if content_length > 0:
                return existing_size + content_length
        return content_length

    def _build_download_request(self, partial_path: Path) -> tuple[dict[str, str], bool, int]:
        """根据已有分片文件生成下载请求参数。"""
        existing_size = partial_path.stat().st_size if partial_path.exists() else 0
        headers: dict[str, str] = {}
        is_resume = existing_size > 0
        if is_resume:
            headers["Range"] = f"bytes={existing_size}-"
        return headers, is_resume, existing_size


class DownloadPausedError(Exception):
    """下载被主动暂停。"""


class DownloadCanceledError(Exception):
    """下载被主动取消。"""


class GithubDownloader(DownloaderBase):
    """执行下载 Github文件 的任务

    可以根据网络环境自动切换下载源, 支持多镜像源下载
    """

    def __init__(self, url: QUrl) -> None:
        """初始化下载器

        Args:
            url (QUrl): 下载链接
        """
        super().__init__()
        self.url: QUrl = url
        self.path: Path = it(PathFunc).tmp_path
        self.file_name = self.url.fileName()
        self.mirror_urls = [QUrl(f"{mirror.toString()}/{self.url.toString()}") for mirror in Urls.MIRROR_SITE.value]

    def run(self) -> None:
        """运行下载 NapCatQQ 的任务"""
        target_path = self.path / self.file_name
        if not self.try_acquire_target(target_path):
            return

        final_progress_status = ProgressRingStatus.INDETERMINATE

        logger.info(
            f"开始 Github 下载任务: file={self.file_name}, source={summarize_url(self.url.toString())}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        try:
            # 显示进度环为不确定进度环
            self.progress_ring_toggle_signal.emit(ProgressRingStatus.INDETERMINATE)

            # 检查网络环境
            if self.check_network():
                # 如果网络环境好, 则直接下载
                if self.download():
                    self.download_finish_signal.emit()  # 发送下载完成信号
                    return  # 下载成功, 直接返回

            for mirror_url in self.mirror_urls:
                self.url = QUrl(mirror_url)
                logger.warning(
                    f"切换下载镜像重试: file={self.file_name}, source={summarize_url(self.url.toString())}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                if self.download():
                    self.download_finish_signal.emit()  # 发送下载完成信号
                    return

            self.status_label_signal.emit(self.tr("下载失败"))
            self.error_finsh_signal.emit()
        except DownloadPausedError:
            final_progress_status = ProgressRingStatus.NONE
            self.status_label_signal.emit(self.tr("下载已暂停"))
            logger.info(f"Github 下载已暂停: file={self.file_name}", LogType.NETWORK, LogSource.CORE)
            self.download_paused_signal.emit()
        except DownloadCanceledError:
            final_progress_status = ProgressRingStatus.NONE
            self.status_label_signal.emit(self.tr("下载已取消"))
            logger.info(f"Github 下载已取消: file={self.file_name}", LogType.NETWORK, LogSource.CORE)
            self.download_canceled_signal.emit()
        finally:
            if final_progress_status != ProgressRingStatus.NONE:
                self.progress_ring_toggle_signal.emit(final_progress_status)
            self.release_target(target_path)

    def download(self) -> bool:
        """下载文件"""
        target_path = self.path / self.file_name
        partial_path = target_path.with_name(f"{target_path.name}.part")
        try:
            headers, is_resume, existing_size = self._build_download_request(partial_path)
            self.status_label_signal.emit(
                self.tr(f"继续下载 {self.file_name} ~ ") if is_resume else self.tr(f" 开始下载 {self.file_name} ~ ")
            )
            with httpx.stream("GET", self.url.url(), follow_redirects=True, headers=headers) as response:
                response.raise_for_status()
                total_size = self._resolve_total_size(response, existing_size)
                if total_size == 0:
                    # 尝试获取文件大小
                    self.status_label_signal.emit(self.tr("无法获取文件大小"))
                    return False

                # 设置进度条为 进度模式
                self.progress_ring_toggle_signal.emit(ProgressRingStatus.DETERMINATE)

                use_resume_mode = is_resume and getattr(response, "status_code", 200) == 206
                if partial_path.exists() and not use_resume_mode and not self.safe_unlink(partial_path):
                    self.status_label_signal.emit(self.tr("检测到上一次下载仍未结束，请稍候重试"))
                    return False

                current_bytes = existing_size if use_resume_mode else 0
                write_mode = "ab" if use_resume_mode else "wb"

                with open(partial_path, write_mode) as file:
                    self.status_label_signal.emit(
                        self.tr(f"正在继续下载 {self.file_name} ~ ") if use_resume_mode else self.tr(f"正在下载 {self.file_name} ~ ")
                    )
                    for chunk in response.iter_bytes():
                        self._ensure_not_interrupted(partial_path)
                        file.write(chunk)  # 写入字节
                        current_bytes += len(chunk)
                        self.download_progress_signal.emit(int((current_bytes / total_size) * 100))  # 设置进度条

                partial_path.replace(target_path)

            # 下载完成
            self.status_label_signal.emit(self.tr("下载完成"))
            logger.info(
                (
                    "Github 下载完成: "
                    f"file={self.file_name}, bytes={total_size}, "
                    f"output={summarize_path(target_path)}"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            return True

        except DownloadPausedError:
            raise
        except DownloadCanceledError:
            self.safe_unlink(partial_path)
            raise
        except (httpx.RequestError, httpx.HTTPStatusError, PermissionError, OSError) as e:
            self.safe_unlink(partial_path)
            logger.error(
                f"Github 下载失败: file={self.file_name}, source={summarize_url(self.url.toString())}, error={e}"
            )
        except Exception as exc:
            self.safe_unlink(partial_path)
            logger.error(
                f"Github 下载发生未知错误: file={self.file_name}, source={summarize_url(self.url.toString())}, error={exc}"
            )
            raise
        return False

    def check_network(self) -> bool:
        """检查网络能否正常访问 Github"""
        try:
            self.status_label_signal.emit(self.tr("检查网络环境"))
            # 如果 3 秒内能访问到 Github 表示网络环境非常奈斯
            response = httpx.head("https://objects.githubusercontent.com", timeout=3)
            if response.status_code == 200:
                self.status_label_signal.emit(self.tr("网络环境良好"))
                return True
            else:
                self.status_label_signal.emit(self.tr("网络环境较差"))
                return False
        except httpx.RequestError as e:
            self.status_label_signal.emit(self.tr(f"网络检查失败: {e}"))
            return False


class QQDownloader(DownloaderBase):
    """执行下载 QQ 的任务"""

    def __init__(self, url: QUrl) -> None:
        super().__init__()
        self.url: QUrl = url
        self.path: Path = it(PathFunc).tmp_path

    def run(self) -> None:
        """运行下载 QQ 的任务"""
        final_progress_status = ProgressRingStatus.INDETERMINATE
        logger.info(
            f"开始 QQ 下载任务: file={self.url.fileName()}, source={summarize_url(self.url.toString())}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        target_path = self.path / self.url.fileName()
        if not self.try_acquire_target(target_path):
            return

        partial_path = target_path.with_name(f"{target_path.name}.part")

        # 开始下载 QQ
        try:
            headers, is_resume, existing_size = self._build_download_request(partial_path)
            with httpx.stream("GET", self.url.url(), follow_redirects=True, headers=headers) as response:
                response.raise_for_status()

                total_size = self._resolve_total_size(response, existing_size)
                if total_size == 0:
                    # 尝试获取文件大小
                    self.status_label_signal.emit(self.tr("无法获取文件大小"))
                    self.error_finsh_signal.emit()
                    return

                # 设置进度条为 进度模式
                self.progress_ring_toggle_signal.emit(ProgressRingStatus.DETERMINATE)

                use_resume_mode = is_resume and getattr(response, "status_code", 200) == 206
                if partial_path.exists() and not use_resume_mode and not self.safe_unlink(partial_path):
                    self.status_label_signal.emit(self.tr("检测到上一次下载仍未结束，请稍候重试"))
                    self.error_finsh_signal.emit()
                    return

                current_bytes = existing_size if use_resume_mode else 0
                write_mode = "ab" if use_resume_mode else "wb"

                with open(partial_path, write_mode) as file:
                    self.status_label_signal.emit(self.tr("正在继续下载 QQ ~ ") if use_resume_mode else self.tr("正在下载 QQ ~ "))
                    for chunk in response.iter_bytes():
                        self._ensure_not_interrupted(partial_path)
                        file.write(chunk)  # 写入字节
                        current_bytes += len(chunk)
                        self.download_progress_signal.emit(int((current_bytes / total_size) * 100))  # 设置进度条

                partial_path.replace(target_path)

            # 下载完成
            self.download_finish_signal.emit()  # 发送下载完成信号
            self.status_label_signal.emit(self.tr("下载完成"))
            logger.info(
                (
                    "QQ 下载完成: "
                    f"file={self.url.fileName()}, bytes={total_size}, "
                    f"output={summarize_path(target_path)}"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )

        except (httpx.RequestError, httpx.HTTPStatusError, PermissionError, OSError) as e:
            self.safe_unlink(partial_path)
            self.status_label_signal.emit(self.tr(f"下载失败: {e}"))
            logger.error(
                f"QQ 下载失败: file={self.url.fileName()}, source={summarize_url(self.url.toString())}, error={e}"
            )
            self.error_finsh_signal.emit()
        except DownloadPausedError:
            final_progress_status = ProgressRingStatus.NONE
            self.status_label_signal.emit(self.tr("下载已暂停"))
            logger.info(f"QQ 下载已暂停: file={self.url.fileName()}", LogType.NETWORK, LogSource.CORE)
            self.download_paused_signal.emit()
        except DownloadCanceledError:
            final_progress_status = ProgressRingStatus.NONE
            self.safe_unlink(partial_path)
            self.status_label_signal.emit(self.tr("下载已取消"))
            logger.info(f"QQ 下载已取消: file={self.url.fileName()}", LogType.NETWORK, LogSource.CORE)
            self.download_canceled_signal.emit()
        except Exception as exc:
            self.safe_unlink(partial_path)
            logger.error(
                f"QQ 下载发生未知错误: file={self.url.fileName()}, source={summarize_url(self.url.toString())}, error={exc}"
            )
            raise

        finally:
            # 无论是否出错,都会重置
            if final_progress_status != ProgressRingStatus.NONE:
                self.progress_ring_toggle_signal.emit(final_progress_status)
            self.release_target(target_path)

