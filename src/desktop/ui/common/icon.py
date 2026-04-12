# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum
from pathlib import Path
from typing import Callable

# 第三方库导入
from PySide6.QtCore import QFile, QRectF, QSize, Qt
from PySide6.QtGui import QColor, QIcon, QImage, QPainter, QPixmap
from qfluentwidgets.common import FluentIconBase, Theme, getIconColor
from qfluentwidgets.common.icon import SvgIconEngine, drawSvgIcon

# 项目内模块导入
from src.core.config import cfg


def _mix_color(source: QColor, target: QColor | str, ratio: float) -> QColor:
    """按比例混合两种颜色。"""
    source = QColor(source)
    target = QColor(target)
    ratio = max(0.0, min(1.0, ratio))
    return QColor(
        round(source.red() * (1 - ratio) + target.red() * ratio),
        round(source.green() * (1 - ratio) + target.green() * ratio),
        round(source.blue() * (1 - ratio) + target.blue() * ratio),
        round(source.alpha() * (1 - ratio) + target.alpha() * ratio),
    )


class PlaceholderThemeSvgIcon(FluentIconBase):
    """支持占位色替换的 SVG 图标。"""

    def __init__(
        self,
        resource_path: str,
        source_path: str | Path,
        palette_factory: Callable[[QColor], dict[str, QColor | str]],
    ) -> None:
        self._resource_path = resource_path
        self._source_path = Path(source_path)
        self._palette_factory = palette_factory
        self._svg_template: str | None = None

    def path(self, theme=Theme.AUTO) -> str:
        return self._resource_path

    def icon(self, theme=Theme.AUTO, color: QColor = None) -> QIcon:
        return QIcon(SvgIconEngine(self._rendered_svg(color)))

    def render(self, painter, rect, theme=Theme.AUTO, indexes=None, **attributes):
        drawSvgIcon(self._rendered_svg(attributes.get("fill")).encode("utf-8"), painter, QRectF(rect))

    def pixmap(self, size: QSize | tuple[int, int], color: QColor | str | None = None) -> QPixmap:
        if isinstance(size, tuple):
            size = QSize(*size)

        image = QImage(size, QImage.Format.Format_ARGB32)
        image.fill(Qt.GlobalColor.transparent)

        painter = QPainter(image)
        painter.setRenderHints(QPainter.RenderHint.Antialiasing | QPainter.RenderHint.SmoothPixmapTransform)
        drawSvgIcon(self._rendered_svg(color).encode("utf-8"), painter, QRectF(0, 0, size.width(), size.height()))
        painter.end()

        return QPixmap.fromImage(image)

    def _read_svg_template(self) -> str:
        if self._svg_template is not None:
            return self._svg_template

        if self._source_path.exists():
            self._svg_template = self._source_path.read_text(encoding="utf-8")
        else:
            resource_file = QFile(self._resource_path)
            if not resource_file.open(QFile.ReadOnly | QFile.Text):
                raise FileNotFoundError(f"无法读取 SVG 资源: {self._resource_path}")

            try:
                self._svg_template = bytes(resource_file.readAll()).decode("utf-8")
            finally:
                resource_file.close()

        return self._svg_template

    def _rendered_svg(self, base_color: QColor | str | None = None) -> str:
        theme_color = QColor(base_color) if base_color else QColor(cfg.get(cfg.themeColor))
        if not theme_color.isValid():
            theme_color = QColor(cfg.get(cfg.themeColor))

        svg = self._read_svg_template()
        for placeholder, color in self._palette_factory(theme_color).items():
            svg = svg.replace(placeholder, QColor(color).name())

        return svg


class NapCatDesktopIcon(FluentIconBase, Enum):
    """主窗体所需要的图标"""

    QQ = "qq"
    LOG = "log"
    GOOD = "good"

    def path(self, theme=Theme.AUTO) -> str:
        return f":mono_icon/icon/mono_icon/{getIconColor(theme)}/{self.value}.svg"


class StaticIcon(FluentIconBase, Enum):
    """静态图标"""

    LOGO = "logo"
    NAPCAT = "napcat"

    def path(self, theme=Theme.AUTO) -> str:
        return f":color_icon/icon/color_icon/{self.value}.png"


class SvgStaticIcon(FluentIconBase, Enum):
    """Svg 静态图标"""

    CAT_GIRL = "cat_girl"

    def path(self, theme=Theme.AUTO) -> str:
        return f":color_icon/icon/color_icon/{self.value}.svg"

    def themed(self) -> FluentIconBase:
        themed_icons = {
            SvgStaticIcon.CAT_GIRL: CAT_GIRL_THEME_ICON,
        }
        return themed_icons.get(self, self)


CAT_GIRL_PLACEHOLDERS = {
    "outline": "#3a5166",
    "shadow": "#466277",
    "dark": "#527388",
    "mid_dark": "#5e8499",
    "base": "#6a95aa",
    "light": "#76a6bb",
}


def _cat_girl_palette(theme_color: QColor) -> dict[str, QColor]:
    return {
        CAT_GIRL_PLACEHOLDERS["outline"]: _mix_color(theme_color, "#000000", 0.52),
        CAT_GIRL_PLACEHOLDERS["shadow"]: _mix_color(theme_color, "#000000", 0.38),
        CAT_GIRL_PLACEHOLDERS["dark"]: _mix_color(theme_color, "#000000", 0.24),
        CAT_GIRL_PLACEHOLDERS["mid_dark"]: _mix_color(theme_color, "#000000", 0.14),
        CAT_GIRL_PLACEHOLDERS["base"]: QColor(theme_color),
        CAT_GIRL_PLACEHOLDERS["light"]: _mix_color(theme_color, "#ffffff", 0.32),
    }


CAT_GIRL_THEME_ICON = PlaceholderThemeSvgIcon(
    resource_path=":color_icon/icon/color_icon/cat_girl.svg",
    source_path=Path(__file__).resolve().parents[2] / "resource" / "icon" / "color_icon" / "cat_girl.svg",
    palette_factory=_cat_girl_palette,
)
