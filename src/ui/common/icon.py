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
    """按比例混合两种颜色. """
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
    """支持占位色替换的 SVG 图标. """

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


# ---------------------------------------------------------------------------
# Provider Icon
# ---------------------------------------------------------------------------

from PySide6.QtSvg import QSvgRenderer  # noqa: E402

# provider_id → canonical icon_id 别名映射
_PROVIDER_ALIASES: dict[str, str] = {
    "silicon": "siliconcloud",
    "siliconflow": "siliconcloud",
    "openai": "openai",
    "anthropic": "anthropic",
    "google": "google",
    "gemini": "gemini",
    "azure": "azure",
    "azure-openai": "azure",
    "mistral": "mistral",
    "cohere": "cohere",
    "groq": "groq",
    "perplexity": "perplexity",
    "together": "together",
    "togetherai": "together",
    "fireworks": "fireworks",
    "moonshot": "kimi",
    "kimi": "kimi",
    "zhipu": "zhipu",
    "glm": "zhipu",
    "chatglm": "chatglm",
    "baichuan": "baichuan",
    "minimax": "minimax",
    "yi": "yi",
    "lingyiwanwu": "yi",
    "qwen": "qwen",
    "tongyi": "qwen",
    "dashscope": "qwen",
    "doubao": "doubao",
    "volcengine": "volcengine",
    "spark": "spark",
    "xunfei": "spark",
    "ollama": "ollama",
    "huggingface": "huggingface",
    "replicate": "replicate",
    "cloudflare": "cloudflare",
    "novita": "novita",
    "openrouter": "openrouter",
    "nvidia": "nvidia",
    "meta": "meta",
    "claude": "claude",
    "cerebras": "cerebras",
    "sambanova": "sambanova",
    "stepfun": "stepfun",
    "deepseek": "deepseek",
}


class ProviderIcon:
    """供应商图标加载器.

    从 Qt 资源系统加载供应商图标, 所有图标统一裁剪为圆形:
    - SVG: :/provider_icon/icon/provider_icon/{icon_id}-color.svg
    - PNG: :/provider_icon/icon/provider_icon/{icon_id}.png
    - 别名解析
    - 首字母圆形头像回退
    - 内存缓存
    """

    _cache: dict[str, QPixmap] = {}
    _ICON_SIZE = 20
    _PREFIX = ":/provider_icon/icon/provider_icon"

    @classmethod
    def get_icon(cls, provider_id: str, display_name: str = "") -> QPixmap:
        """获取供应商图标 (20×20 圆形).

        Args:
            provider_id: 供应商唯一标识符.
            display_name: 显示名称, 用于首字母头像回退.
        """
        cache_key = provider_id.lower().strip()
        if cache_key in cls._cache:
            return cls._cache[cache_key]

        pixmap = cls._resolve_icon(cache_key)
        if pixmap is None:
            pixmap = cls._create_initial_avatar(display_name or provider_id)
        else:
            pixmap = cls._clip_circle(pixmap)

        cls._cache[cache_key] = pixmap
        return pixmap

    @classmethod
    def clear_cache(cls) -> None:
        """清除图标缓存."""
        cls._cache.clear()

    @classmethod
    def _clip_circle(cls, source: QPixmap) -> QPixmap:
        """将任意 QPixmap 裁剪为圆形.

        参考 qfluentwidgets AvatarWidget._drawImageAvatar 的实现:
        使用 QPainterPath.addEllipse + setClipPath 进行圆形裁剪.
        """
        from PySide6.QtGui import QPainterPath

        size = cls._ICON_SIZE
        # 先缩放到目标尺寸 (center crop)
        scaled = source.scaled(
            QSize(size, size),
            Qt.AspectRatioMode.KeepAspectRatioByExpanding,
            Qt.TransformationMode.SmoothTransformation,
        )
        # 居中裁剪
        x = (scaled.width() - size) // 2
        y = (scaled.height() - size) // 2
        cropped = scaled.copy(x, y, size, size)

        # 圆形裁剪
        result = QPixmap(size, size)
        result.fill(Qt.GlobalColor.transparent)
        painter = QPainter(result)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        path = QPainterPath()
        path.addEllipse(0.0, 0.0, float(size), float(size))
        painter.setClipPath(path)
        painter.drawPixmap(0, 0, cropped)
        painter.end()
        return result

    @classmethod
    def _resolve_icon(cls, normalized_id: str) -> QPixmap | None:
        """按优先级查找图标: 直接SVG → 直接PNG → 别名SVG → 别名PNG."""
        pixmap = cls._try_load_svg(f"{normalized_id}-color.svg")
        if pixmap:
            return pixmap

        pixmap = cls._try_load_png(f"{normalized_id}.png")
        if pixmap:
            return pixmap

        canonical_id = _PROVIDER_ALIASES.get(normalized_id)
        if canonical_id and canonical_id != normalized_id:
            pixmap = cls._try_load_svg(f"{canonical_id}-color.svg")
            if pixmap:
                return pixmap
            pixmap = cls._try_load_png(f"{canonical_id}.png")
            if pixmap:
                return pixmap

        return None

    @classmethod
    def _try_load_svg(cls, filename: str) -> QPixmap | None:
        """从 Qt 资源系统加载 SVG."""
        from PySide6.QtCore import QFile

        resource_path = f"{cls._PREFIX}/{filename}"
        if not QFile.exists(resource_path):
            return None
        renderer = QSvgRenderer(resource_path)
        if not renderer.isValid():
            return None
        size = cls._ICON_SIZE
        pixmap = QPixmap(QSize(size, size))
        pixmap.fill(Qt.GlobalColor.transparent)
        painter = QPainter(pixmap)
        renderer.render(painter)
        painter.end()
        return pixmap

    @classmethod
    def _try_load_png(cls, filename: str) -> QPixmap | None:
        """从 Qt 资源系统加载 PNG."""
        from PySide6.QtCore import QFile

        resource_path = f"{cls._PREFIX}/{filename}"
        if not QFile.exists(resource_path):
            return None
        pixmap = QPixmap(resource_path)
        if pixmap.isNull():
            return None
        return pixmap.scaled(
            cls._ICON_SIZE, cls._ICON_SIZE,
            Qt.AspectRatioMode.KeepAspectRatio,
            Qt.TransformationMode.SmoothTransformation,
        )

    @classmethod
    def _create_initial_avatar(cls, name: str) -> QPixmap:
        """首字母圆形头像回退 (已经是圆形, 无需再裁剪)."""
        size = cls._ICON_SIZE
        pixmap = QPixmap(size, size)
        pixmap.fill(Qt.GlobalColor.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        hue = hash(name) % 360
        bg_color = QColor.fromHsv(hue, 120, 180)
        painter.setBrush(bg_color)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawEllipse(0, 0, size, size)
        initial = name[0].upper() if name else "?"
        painter.setPen(QColor(255, 255, 255))
        font = painter.font()
        font.setPointSize(9)
        font.setBold(True)
        painter.setFont(font)
        painter.drawText(pixmap.rect(), Qt.AlignmentFlag.AlignCenter, initial)
        painter.end()
        return pixmap
