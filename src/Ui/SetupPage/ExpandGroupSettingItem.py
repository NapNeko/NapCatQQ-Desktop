# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import (
    Slider,
    ComboBox,
    LineEdit,
    BodyLabel,
    ConfigItem,
    FluentIcon,
    PushButton,
    SwitchButton,
    FluentIconBase,
    RangeConfigItem,
)
from PySide6.QtGui import QIcon
from PySide6.QtCore import Qt, QDir, Slot, Signal
from PySide6.QtWidgets import QWidget, QFileDialog, QHBoxLayout

# 项目内模块导入
from src.Core.Config import cfg


class ItemBase(QWidget):
    """
    ## 设置卡片的基类
    """

    def __init__(self, title: str, parent=None) -> None:
        """
        ## 初始化

        ## 参数
        ----------
        - title: str
            - 标题

        - parent: QWidget
            - 父窗口
        """
        super().__init__(parent=parent)

        # 创建控件
        self.titleLabel = BodyLabel(title, self)
        self.hBoxLayout = QHBoxLayout(self)

        # 布局调整
        self.setFixedHeight(64)
        self.hBoxLayout.addWidget(self.titleLabel, 0, Qt.AlignmentFlag.AlignLeft)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.setContentsMargins(32, 12, 32, 12)

    def setTitle(self, title: str) -> None:
        """
        ## 设置标题

        ## 参数
        - title: str
            - 标题
        """
        self.titleLabel.setText(title)

    def setValue(self, value) -> None:
        """
        ## 设置值(继承实现)

        ## 参数
        - value
            - 值
        """
        pass


class LineEditItem(ItemBase):
    """
    ## 展开设置卡片的子组件
    """

    def __init__(self, configItem: ConfigItem, title: str, /, placeholder_text: str = "", parent=None):
        super().__init__(title, parent=parent)
        self.configItem = configItem
        self.lineEdit = LineEdit(self)
        self.lineEdit.setPlaceholderText(placeholder_text)

        if configItem:
            self.setValue(cfg.get(configItem))

        # 设置控件
        self.lineEdit.editingFinished.connect(lambda: self.setValue(self.lineEdit.text()))
        self.lineEdit.setFixedWidth(175)

        # 添加到布局
        self.hBoxLayout.addWidget(self.lineEdit, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    @Slot(str)
    def setValue(self, value: str) -> None:
        """
        ## 设置值
        """
        if self.configItem:
            cfg.set(self.configItem, value)

        self.lineEdit.setText(value)


class SwitchItem(ItemBase):
    """
    ## 展开设置卡片的子组件
    """

    checkedChanged = Signal(bool)

    def __init__(self, configItem: ConfigItem, title: str, parent=None) -> None:
        """
        ## 初始化

        ## 参数
        - title: str
            - 标题
        - configItem: ConfigItem
            - 配置项
        - parent: QWidget
            - 父窗口
        """
        super().__init__(title, parent=parent)
        self.configItem = configItem
        self.switchButton = SwitchButton(self)

        if configItem:
            self.setValue(cfg.get(configItem))
            configItem.valueChanged.connect(self.setValue)

        # 添加到布局
        self.hBoxLayout.addWidget(self.switchButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(2)

        self.switchButton.checkedChanged.connect(self._onCheckedChanged)

    def _onCheckedChanged(self, value: bool) -> None:
        """
        ## 选中状态变更
        """
        self.setValue(value)
        self.checkedChanged.emit(value)

    @Slot(bool)
    def setValue(self, value: bool) -> None:
        """
        ## 设置值
        """
        if self.configItem:
            cfg.set(self.configItem, value)

        self.switchButton.setChecked(value)

    def setChecked(self, value: bool) -> None:
        """
        ## 设置选中状态
        """
        self.setValue(value)

    def isChecked(self) -> bool:
        """
        ## 获取选中状态
        """
        return self.switchButton.isChecked()


class ComboBoxItem(ItemBase):
    """
    ## 展开设置卡片的子组件
    """

    def __init__(self, configItem: ConfigItem, title: str, texts=None, parent=None) -> None:
        """
        ## 初始化
        """
        super().__init__(title, parent=parent)
        self.configItem = configItem
        self.comboBox = ComboBox(self)
        self.hBoxLayout.addWidget(self.comboBox, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

        # 添加选项
        self.optionToText = {option: text for option, text in zip(configItem.options, texts)}
        for text, option in zip(texts, configItem.options):
            self.comboBox.addItem(text, userData=option)

        # 设置控件
        self.comboBox.setCurrentText(self.optionToText[cfg.get(configItem)])
        self.comboBox.currentIndexChanged.connect(self._currentIndexChanged)
        self.comboBox.setFixedWidth(135)

    @Slot(int)
    def _currentIndexChanged(self, index: int) -> None:
        """
        ## 选项变更
        """
        cfg.set(self.configItem, self.comboBox.itemData(index))

    def setValue(self, value) -> None:
        """
        ## 设置值
        """
        if value not in self.optionToText:
            return

        self.comboBox.setCurrentText(self.optionToText[value])
        cfg.set(self.configItem, value)


class RangeItem(ItemBase):
    """
    ## 展开设置卡片的子组件
    """

    valueChanged = Signal(int)

    def __init__(self, configItem: RangeConfigItem, title: str, parent=None) -> None:
        """
        ## 初始化
        """
        super().__init__(title, parent=parent)
        self.configItem = configItem
        self.slider = Slider(Qt.Orientation.Horizontal, self)
        self.valueLabel = BodyLabel(self)

        # 调整控件
        self.valueLabel.setObjectName("valueLabel")
        self.slider.setMinimumWidth(260)
        self.slider.setRange(*configItem.range)
        self.slider.setValue(configItem.value)
        self.slider.setFixedWidth(200)
        self.valueLabel.setNum(configItem.value)

        # 调整布局
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(self.valueLabel, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(6)
        self.hBoxLayout.addWidget(self.slider, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

        # 信号连接
        configItem.valueChanged.connect(self.setValue)
        self.slider.valueChanged.connect(self._onValueChanged)

    @Slot(int)
    def _onValueChanged(self, value: int) -> None:
        """
        ## 值变更
        """
        self.setValue(value)
        self.valueChanged.emit(value)

    def setValue(self, value: int) -> None:
        """
        ## 设置值
        """
        cfg.set(self.configItem, value)
        self.valueLabel.setNum(value)
        self.valueLabel.adjustSize()
        self.slider.setValue(value)


class FileItem(ItemBase):
    """
    ## 展开设置卡片的子组件
    """
    fileChanged = Signal(str)  # file Path

    def __init__(self, configItem: ConfigItem, title: str, parent=None) -> None:
        """
        ## 初始化
        """
        super().__init__(title, parent=parent)
        self.configItem = configItem
        self.title = title
        self.button = PushButton(self.tr("选择文件"), self, FluentIcon.FOLDER_ADD)
        self.button.clicked.connect(self._onButtonClicked)

        # 调整控件
        self.button.setFixedWidth(135)
        if cfg.get(self.configItem):
            self.titleLabel.setText(f"{self.title}: {cfg.get(self.configItem)}")

        # 添加到布局
        self.hBoxLayout.addWidget(self.button, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    @Slot()
    def _onButtonClicked(self) -> None:
        """
        ## 按钮点击
        """
        file, _ = QFileDialog.getOpenFileName(
            self, self.tr("选择文件"), QDir.homePath(), "Image Files (*.png *.jpg *.jpeg)"
        )

        if file:
            cfg.set(self.configItem, file)
            self.titleLabel.setText(f"{self.title}: {file}")
            self.fileChanged.emit(file)
