# -*- coding: utf-8 -*-
# 标准库导入
import sys
import datetime

# 第三方库导入
from creart import it
from loguru import logger

# 项目内模块导入
from src.Core.Utils.PathFunc import PathFunction

# 获取路径
TIME = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
INFO_LOG = PathFunction.log_info_path / f"INFO.{TIME}.log"
DEBUG_LOG = PathFunction.log_debug_path / f"DEBUG.{TIME}.log"

CUSTOM_FORMAT = "<level>{time:YYYY-MM-DD HH:mm:ss}</level> | <level>{level: <8}</level> | <level>{message}</level>"


class Logger:

    def __init__(self):

        # 移除原有过滤器
        logger.remove()
        # 添加自定义的 logger
        logger.add(sys.stderr, level="INFO", format=CUSTOM_FORMAT)
        logger.add(INFO_LOG, level="INFO", format=CUSTOM_FORMAT, enqueue=True)
        logger.add(DEBUG_LOG, level="DEBUG", format=CUSTOM_FORMAT, enqueue=True)

        # 清理旧日志
        self.cleanup_logs(PathFunction.log_info_path, "INFO.")
        self.cleanup_logs(PathFunction.log_debug_path, "DEBUG.")

    @staticmethod
    def cleanup_logs(log_path, log_prefix) -> None:
        log_files = sorted(
            (f for f in log_path.iterdir() if f.is_file() and f.name.startswith(log_prefix)),
            key=lambda f: f.stat().st_mtime,
            reverse=True,
        )
        for log_file in log_files[10:]:
            try:
                log_file.unlink()
            except Exception as e:
                logger.error(f"清理日志文件失败 {log_file}: {e}")
