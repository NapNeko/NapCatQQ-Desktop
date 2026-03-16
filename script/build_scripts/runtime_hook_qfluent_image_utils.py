# -*- coding: utf-8 -*-
"""替换 qfluentwidgets 的重型 image_utils 依赖。

原实现依赖 numpy/scipy；这里用 Pillow 提供最小兼容实现，
避免把整套科学计算库打进发布包。
"""

# 标准库导入
import sys
import types
from io import BytesIO
from math import floor
from typing import Union

# 第三方库导入
from colorthief import ColorThief
from PIL import Image, ImageFilter, ImageEnhance
from PySide6.QtCore import QBuffer, QIODevice
from PySide6.QtGui import QImage, QPixmap


def _from_qpixmap(im: Union[QImage, QPixmap]) -> Image.Image:
    buffer = QBuffer()
    buffer.open(QIODevice.OpenModeFlag.ReadWrite)
    if im.hasAlphaChannel():
        im.save(buffer, "png")
    else:
        im.save(buffer, "ppm")

    bio = BytesIO()
    bio.write(buffer.data())
    buffer.close()
    bio.seek(0)
    return Image.open(bio)


def _gaussian_blur(image, blurRadius=18, brightFactor=1, blurPicSize=None):
    if isinstance(image, str) and not image.startswith(":"):
        pil_image = Image.open(image)
    else:
        pil_image = _from_qpixmap(QPixmap(image))

    if blurPicSize:
        w, h = pil_image.size
        ratio = min(blurPicSize[0] / w, blurPicSize[1] / h)
        w_, h_ = w * ratio, h * ratio
        if w_ < w:
            pil_image = pil_image.resize((int(w_), int(h_)))

    pil_image = pil_image.convert("RGBA")
    pil_image = pil_image.filter(ImageFilter.GaussianBlur(radius=blurRadius))
    if brightFactor != 1:
        pil_image = ImageEnhance.Brightness(pil_image).enhance(brightFactor)

    data = pil_image.tobytes("raw", "RGBA")
    qimage = QImage(data, pil_image.width, pil_image.height, pil_image.width * 4, QImage.Format.Format_RGBA8888)
    return QPixmap.fromImage(qimage.copy())


class _DominantColor:
    """兼容 qfluentwidgets 的 DominantColor API。"""

    @classmethod
    def getDominantColor(cls, imagePath):
        if imagePath.startswith(":"):
            return (24, 24, 24)

        color_thief = ColorThief(imagePath)
        if max(color_thief.image.size) > 400:
            color_thief.image = color_thief.image.resize((400, 400))

        palette = cls.__adjustPaletteValue(color_thief.get_palette(quality=9))
        for rgb in palette[:]:
            h, s, _ = cls.rgb2hsv(rgb)
            if h < 0.02:
                palette.remove(rgb)
                if len(palette) <= 2:
                    break

        palette = palette[:5]
        palette.sort(key=lambda rgb: cls.colorfulness(*rgb), reverse=True)
        return palette[0] if palette else (24, 24, 24)

    @classmethod
    def __adjustPaletteValue(cls, palette):
        new_palette = []
        for rgb in palette:
            h, s, v = cls.rgb2hsv(rgb)
            if v > 0.9:
                factor = 0.8
            elif 0.8 < v <= 0.9:
                factor = 0.9
            elif 0.7 < v <= 0.8:
                factor = 0.95
            else:
                factor = 1
            new_palette.append(cls.hsv2rgb(h, s, v * factor))
        return new_palette

    @staticmethod
    def rgb2hsv(rgb):
        r, g, b = [i / 255 for i in rgb]
        mx = max(r, g, b)
        mn = min(r, g, b)
        df = mx - mn
        if mx == mn:
            h = 0
        elif mx == r:
            h = (60 * ((g - b) / df) + 360) % 360
        elif mx == g:
            h = (60 * ((b - r) / df) + 120) % 360
        else:
            h = (60 * ((r - g) / df) + 240) % 360
        s = 0 if mx == 0 else df / mx
        return (h, s, mx)

    @staticmethod
    def hsv2rgb(h, s, v):
        h60 = h / 60.0
        h60f = floor(h60)
        hi = int(h60f) % 6
        f = h60 - h60f
        p = v * (1 - s)
        q = v * (1 - f * s)
        t = v * (1 - (1 - f) * s)
        r, g, b = 0, 0, 0
        if hi == 0:
            r, g, b = v, t, p
        elif hi == 1:
            r, g, b = q, v, p
        elif hi == 2:
            r, g, b = p, v, t
        elif hi == 3:
            r, g, b = p, q, v
        elif hi == 4:
            r, g, b = t, p, v
        elif hi == 5:
            r, g, b = v, p, q
        return (int(r * 255), int(g * 255), int(b * 255))

    @staticmethod
    def colorfulness(r: int, g: int, b: int):
        rg = abs(r - g)
        yb = abs(0.5 * (r + g) - b)
        return (rg**2 + yb**2) ** 0.5


module = types.ModuleType("qfluentwidgets.common.image_utils")
module.gaussianBlur = _gaussian_blur
module.fromqpixmap = _from_qpixmap
module.DominantColor = _DominantColor
sys.modules[module.__name__] = module
