# -*- coding: utf-8 -*-
# NCD 组件页面中的通用组件卡片组件
# 第三方库导入
from qfluentwidgets import SkeletonScreen, SimpleCardWidget, SkeletonPlaceholder
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout

# class CardPage(QWidget):
#     """NCD 组件页面中的通用组件卡片页面"""

#     def __init__(self, parent=None) -> None:
#         """构造函数"""
#         super().__init__(parent)
#         self.createComponent()
#         self.setupLayout()

#     def createComponent(self) -> None:
#         """创建组件"""
#         pass

#     def setupLayout(self) -> None:
#         """设置布局"""
#         pass


# class SkeletonWidget(QWidget):
#     """骨架屏占位符页面"""

#     def __init__(self, parent=None) -> None:
#         super().__init__(parent=parent)

#     def createComponent(self) -> None:
#         """创建组件"""
#         # 创建骨架屏占位符
#         self.imagePlaceholder = SkeletonPlaceholder(8, 8, 8, 8, self)
#         self.titlePlaceholder = SkeletonPlaceholder(8, 8, 8, 8, self)
#         self.textPlaceholder = SkeletonPlaceholder(8, 8, 8, 8, self)


# class CardWidgetBase(SimpleCardWidget):
#     """NCD 组件页面中的通用组件卡片组件"""

#     def __init__(self, parent=None) -> None:
#         """构造函数"""
#         super().__init__(parent)
#         self.createComponent()
#         self.setupComponent()
#         self.setupLayout()

#     def createComponent(self) -> None:
#         """创建组件"""
#         # self.view = SkeletonScreen(SkeletonWidget(), QWidget(), self)

#     def setupComponent(self) -> None:
#         """设置组件"""
#         self.setFixedHeight(200)

#     def setupLayout(self) -> None:
#         """设置布局"""
#         self.hBoxLayout = QHBoxLayout(self)
#         self.hBoxLayout.setContentsMargins(0, 0, 0, 0)
#         self.hBoxLayout.addStretch(1)
