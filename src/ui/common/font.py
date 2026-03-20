# -*- coding: utf-8 -*-
# font_manager.py
from PySide6.QtGui import QFontDatabase

# 项目内模块导入
from src.core.utils.logger import logger


class FontManager:
    _font_loaded = False
    _loaded_families: list[str] = []

    @classmethod
    def initialize_fonts(cls) -> None:
        if not cls._font_loaded:
            cls._load_single_font(":/font/font/JB-MAPLE.ttf")
            cls._font_loaded = True

    @classmethod
    def _load_single_font(cls, font_path: str) -> str:
        """加载单个字体并返回字体家族"""
        if (font_id := QFontDatabase.addApplicationFont(font_path)) == -1:
            logger.error(f"字体加载失败: {font_path}")
            return ""

        if font_families := QFontDatabase.applicationFontFamilies(font_id):
            for family in font_families:
                if family not in cls._loaded_families:
                    cls._loaded_families.append(family)
            logger.trace(f"字体加载成功: {font_families[0]}")
            return font_families[0]

        logger.error(f"未找到字体家族: {font_path}")
        return ""

    @classmethod
    def code_font_families(cls) -> list[str]:
        """返回代码编辑器优先使用的等宽字体栈。"""
        preferred = [family for family in cls._loaded_families if "maple" in family.lower() or "mono" in family.lower()]
        fallback = [
            "Cascadia Mono",
            "Cascadia Code",
            "JetBrains Mono",
            "Consolas",
            "Microsoft YaHei UI",
        ]

        result: list[str] = []
        for family in preferred + fallback:
            if family not in result:
                result.append(family)

        return result
