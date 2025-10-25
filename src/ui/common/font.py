# -*- coding: utf-8 -*-
# font_manager.py
from PySide6.QtGui import QFontDatabase
from qfluentwidgets import setFontFamilies, fontFamilies

# 项目内模块导入
from src.core.utils.logger import logger


class FontManager:
    _font_loaded = False

    @classmethod
    def initialize_fonts(cls) -> None:
        if not cls._font_loaded:

            fonts = fontFamilies()
            fonts.insert(0, cls._load_single_font(":/font/font/JB-MAPLE.ttf"))
            setFontFamilies(fonts)
            cls._font_loaded = True

    @classmethod
    def _load_single_font(cls, font_path: str) -> str:
        """加载单个字体并返回字体家族"""
        if (font_id := QFontDatabase.addApplicationFont(font_path)) == -1:
            logger.error(f"字体加载失败: {font_path}")
            return ""

        if font_families := QFontDatabase.applicationFontFamilies(font_id):
            logger.info(f"字体加载成功: {font_families[0]}")
            return font_families[0]

        logger.error(f"未找到字体家族: {font_path}")
        return ""
