# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import CardWidget, ImageLabel, StrongBodyLabel, SubtitleLabel
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.ui.common.emoticons import FuFuEmoticons

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.guide_window.guide_window import GuideWindow


class AskPage(QWidget):
    """询问用户是新手还是老手页面

    该页面有两个选项, 分别是新手和老手
    选择新手会进入安装向导, 选择老手会直接关闭向导, 进入主程序

    Attributes:
        beginner_card (BeginnerCard): 新手卡片
        experienced_card (ExperiencedCard): 老手卡片
        title_label (SubtitleLabel): 标题标签
        card_layout (QHBoxLayout): 卡片布局
        v_box_layout (QVBoxLayout): 垂直布局
    """

    def __init__(self, parent: "GuideWindow") -> None:
        """初始化

        创建控件, 布局, 信号连接等

        Args:
            parent (GuideWindow): 父窗体
        """
        super().__init__(parent)

        # 创建控件
        self.beginner_card = BeginnerCard(self)
        self.experienced_card = ExperiencedCard(self)
        self.title_label = SubtitleLabel(self.tr("你是新手还是老手？"), self)

        # 布局
        self.card_layout = QHBoxLayout()
        self.card_layout.setContentsMargins(0, 0, 0, 0)
        self.card_layout.setSpacing(0)
        self.card_layout.addStretch(1)
        self.card_layout.addWidget(self.beginner_card)
        self.card_layout.addStretch(1)
        self.card_layout.addWidget(self.experienced_card)
        self.card_layout.addStretch(1)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(0)
        self.v_box_layout.addStretch(1)
        self.v_box_layout.addWidget(self.title_label, 0, Qt.AlignmentFlag.AlignCenter)
        self.v_box_layout.addSpacing(30)
        self.v_box_layout.addLayout(self.card_layout, 0)
        self.v_box_layout.addStretch(1)


class BeginnerCard(CardWidget):
    """新手卡片

    Attributes:
        image_label (ImageLabel): 图片标签
        title_label (StrongBodyLabel): 标题标签
        v_box_layout (QVBoxLayout): 垂直布局
    """

    def __init__(self, parent: AskPage):
        """初始化

        创建控件, 布局, 信号连接等

        Args:
            parent (GuideWindow): 父窗体
        """
        super().__init__(parent)

        # 创建控件
        self.image_label = ImageLabel(FuFuEmoticons.FU_11.path(), self)
        self.title_label = StrongBodyLabel(self.tr("我是新手"), self)

        # 设置属性
        self.setFixedSize(200, 240)
        self.image_label.scaledToHeight(128)

        # 布局
        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(0)
        self.v_box_layout.addStretch(1)
        self.v_box_layout.addWidget(self.image_label, 0, Qt.AlignmentFlag.AlignCenter)
        self.v_box_layout.addSpacing(10)
        self.v_box_layout.addWidget(self.title_label, 0, Qt.AlignmentFlag.AlignCenter)
        self.v_box_layout.addStretch(1)

        # 信号连接
        self.clicked.connect(self.on_click)

    def on_click(self) -> None:
        """点击事件"""
        # 项目内模块导入
        from src.ui.window.guide_window.guide_window import GuideWindow

        GuideWindow().on_next_page()


class ExperiencedCard(CardWidget):
    """老手卡片

    Attributes:
        image_label (ImageLabel): 图片标签
        title_label (StrongBodyLabel): 标题标签
        v_box_layout (QVBoxLayout): 垂直布局
    """

    def __init__(self, parent: AskPage):
        super().__init__(parent)

        # 创建控件
        self.image_label = ImageLabel(FuFuEmoticons.FU_13.path(), self)
        self.title_label = StrongBodyLabel(self.tr("我是老手"), self)

        # 设置属性
        self.setFixedSize(200, 240)
        self.image_label.scaledToHeight(128)

        # 布局
        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(0)
        self.v_box_layout.addStretch(1)
        self.v_box_layout.addWidget(self.image_label, 0, Qt.AlignmentFlag.AlignCenter)
        self.v_box_layout.addSpacing(10)
        self.v_box_layout.addWidget(self.title_label, 0, Qt.AlignmentFlag.AlignCenter)
        self.v_box_layout.addStretch(1)

        # 信号连接
        self.clicked.connect(self.on_click)

    def on_click(self) -> None:
        """点击事件"""
        # 项目内模块导入
        from src.ui.window.guide_window.guide_window import GuideWindow

        GuideWindow().close()
