# -*- coding: utf-8 -*
# 第三方库导入
from pydantic import BaseModel
from qfluentwidgets.common import (
    FluentIcon,
    FluentIconBase,
    FluentStyleSheet,
    isDarkTheme,
    ConfigItem,
    setFontFamilies,
    fontFamilies,
)
from qfluentwidgets.components import (
    ComboBox,
    Flyout,
    IndicatorPosition,
    LineEdit,
    MessageBoxBase,
    PushButton,
    SwitchButton,
    TableWidget,
    TransparentPushButton,
    TransparentToolButton,
    HyperlinkLabel,
)
from qfluentwidgets.components.settings import SettingCard
from qfluentwidgets.components.settings.setting_card import SettingIconWidget
from qfluentwidgets.components.widgets.flyout import FlyoutView
from PySide6.QtCore import QObject, QSize, QStandardPaths, Qt
from PySide6.QtGui import QColor, QPainter
from PySide6.QtWidgets import (
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QSizePolicy,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)
from src.core.config import cfg
from creart import it

# 项目内模块导入
from src.ui.components.code_editor import JsonEditor

"""通用配置卡片

### 说明
- 该模块内包含多种常用的配置卡片,可直接使用
- 包含的配置卡片有:
    - JsonTemplateEditConfigCard: Json 模板编辑卡片
    - LineEditConfigCard: 单行文本输入配置卡片
    - ComboBoxConfigCard: 下拉选择配置卡片
    - SwitchConfigCard: 开关配置卡片
    - FolderConfigCard: 文件夹选择配置卡片
    - ShowDialogCard: 显示对话框卡片

- 每个配置卡片均继承自 SettingCard,并实现了 fill_value, get_value, clear 方法
"""


class JsonTemplateEditConfigCard(QFrame):
    """Json 模板编辑卡片

    ### 模板字符串
    - `${bot_name}`: 显示您在 NCD 中配置的机器人名称
    - `${bot_qq_id}`: 显示机器人的 QQ 号
    - `${disconnect_time}`: 显示为当前的时间(发件时间)

    ### 示例
    ```json
    {
        "msg": "Hello, ${bot_name}! Your QQ ID is ${bot_qq_id}. Current time is ${disconnect_time}."
    }
    ```

    ### 说明
    - 以上 JSON 模板字符串将会被 NCD 自动解析成相应内容插入
    - 点击 "插入模板" 按钮可以查看并插入模板字符串

    """

    TEMPLATE_STRING = [
        {"name": "机器人名称", "string": "${bot_name}", "doc": "显示您在 NCD 中配置的机器人名称"},
        {"name": "机器人QQ号", "string": "${bot_qq_id}", "doc": "显示机器人的 QQ 号"},
        {"name": "当前时间", "string": "${disconnect_time}", "doc": "显示为当前的时间(发件时间)"},
    ]

    def __init__(self, icon: FluentIconBase, title: str, parent: QObject | None = None) -> None:
        super().__init__(parent)

        # 创建控件
        self.icon_label = SettingIconWidget(icon, self)
        self.title_label = QLabel(title, self)
        self.json_text_edit = JsonEditor(self)
        self.editor_font_size_add_button = TransparentToolButton(FluentIcon.ADD, self)
        self.editor_font_size_sub_button = TransparentToolButton(FluentIcon.REMOVE, self)
        self.check_button = TransparentPushButton(FluentIcon.CODE, self.tr("校验 JSON"), self)
        self.insert_template_button = TransparentPushButton(FluentIcon.DICTIONARY_ADD, self.tr("插入模板"), self)
        self.v_box_layout = QVBoxLayout(self)
        self.h_box_layout = QHBoxLayout()

        # 设置属性
        self.setMinimumHeight(300)
        self.setMinimumWidth(400)
        self.icon_label.setFixedSize(16, 16)
        self.json_text_edit.setReadOnly(False)

        # 信号与槽
        self.editor_font_size_add_button.clicked.connect(
            lambda: self.json_text_edit.set_font_size(self.json_text_edit.font_size + 1)
        )
        self.editor_font_size_sub_button.clicked.connect(
            lambda: self.json_text_edit.set_font_size(self.json_text_edit.font_size - 1)
        )
        self.check_button.clicked.connect(self.json_text_edit.check_json)
        self.insert_template_button.clicked.connect(self._on_show_template_flyout)

        # 布局
        self.h_box_layout.setSpacing(0)
        self.h_box_layout.setContentsMargins(0, 0, 0, 0)
        self.h_box_layout.setAlignment(Qt.AlignmentFlag.AlignVCenter)

        self.v_box_layout.setSpacing(0)
        self.v_box_layout.setContentsMargins(16, 16, 16, 16)
        self.v_box_layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        self.h_box_layout.addWidget(self.icon_label, 0, Qt.AlignmentFlag.AlignLeft)
        self.h_box_layout.addSpacing(16)
        self.h_box_layout.addWidget(self.title_label, 0, Qt.AlignmentFlag.AlignLeft)
        self.h_box_layout.addSpacing(16)
        self.h_box_layout.addStretch(1)
        self.h_box_layout.addWidget(self.editor_font_size_sub_button, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addWidget(self.editor_font_size_add_button, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addWidget(self.check_button, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addWidget(self.insert_template_button, 0, Qt.AlignmentFlag.AlignRight)
        self.h_box_layout.addSpacing(8)

        self.v_box_layout.addLayout(self.h_box_layout)
        self.v_box_layout.addSpacing(8)
        self.v_box_layout.addWidget(self.json_text_edit)

        FluentStyleSheet.SETTING_CARD.apply(self)

    def paintEvent(self, e):
        painter = QPainter(self)
        painter.setRenderHints(QPainter.Antialiasing)

        if isDarkTheme():
            painter.setBrush(QColor(255, 255, 255, 13))
            painter.setPen(QColor(0, 0, 0, 50))
        else:
            painter.setBrush(QColor(255, 255, 255, 170))
            painter.setPen(QColor(0, 0, 0, 19))

        painter.drawRoundedRect(self.rect().adjusted(1, 1, -1, -1), 6, 6)

    def fill_value(self, value: str) -> None:
        self.json_text_edit.set_json(str(value))

    def get_value(self) -> str:
        return self.json_text_edit.get_json()

    def clear(self) -> None:
        self.json_text_edit.clear()

    def _on_show_template_flyout(self) -> None:
        """展示模板弹出组件"""
        # 创建视图
        view = FlyoutView(
            title=self.tr("Json 模板"),
            content=self.tr(
                "这是自带的模板字符串以及模板,模板字符串将会被 NCD 自动解析成相应内容插入\n点击表格中的内容即视为执行插入"
            ),
            isClosable=True,
        )

        # 创建相关组件
        table = TableWidget()
        table.setBorderVisible(True)
        table.setBorderRadius(8)
        table.setWordWrap(False)
        table.setRowCount(len(self.TEMPLATE_STRING))
        table.setColumnCount(3)
        table.setHorizontalHeaderLabels(["模板名称", "模板字符串", "介绍"])
        table.verticalHeader().hide()

        # 修改列宽设置 - 前两列根据内容调整，第三列拉伸填充
        table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.ResizeToContents)
        table.horizontalHeader().setSectionResizeMode(2, QHeaderView.ResizeMode.Stretch)

        table.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        table.setMinimumWidth(530)

        # 链接信号
        table.cellClicked.connect(self._on_table_cell_clicked)

        # 填充数据
        for row, item in enumerate(self.TEMPLATE_STRING):
            # 模板名称列
            name_item = QTableWidgetItem(item["name"])
            name_item.setTextAlignment(Qt.AlignCenter)
            table.setItem(row, 0, name_item)

            # 模板字符串列
            string_item = QTableWidgetItem(item["string"])
            string_item.setTextAlignment(Qt.AlignCenter)
            table.setItem(row, 1, string_item)

            # 介绍列
            doc_item = QTableWidgetItem(item["doc"])
            table.setItem(row, 2, doc_item)

        view.addWidget(table)
        view.setMinimumSize(QSize(600, 290))

        # 展示窗口
        widget = Flyout.make(view, self.insert_template_button, self)
        view.closed.connect(widget.close)

    def _on_table_cell_clicked(self, row: int, _: int) -> None:
        """列表点击事件处理"""
        cursor = self.json_text_edit.textCursor()
        cursor.insertText(self.TEMPLATE_STRING[row]["string"])
        self.json_text_edit.setTextCursor(cursor)


class LineEditConfigCard(SettingCard):
    """单行文本输入配置卡片"""

    def __init__(
        self,
        icon: FluentIconBase,
        title: str,
        placeholder_text: str = "",
        content: str | None = None,
        parent: QWidget | None = None,
    ) -> None:
        """初始化

        Args:
            icon (FluentIconBase): 图标
            title (str): 标题
            placeholder_text (str, optional): 占位展示文本. Defaults to "".
            content (str | None, optional): 呢容. Defaults to None.
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(icon, title, content, parent)
        self.lineEdit = LineEdit(self)
        self.lineEdit.setClearButtonEnabled(True)
        self.lineEdit.setPlaceholderText(placeholder_text)
        self.lineEdit.setFixedWidth(165)
        self.hBoxLayout.addWidget(self.lineEdit, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def fill_value(self, value: str | int) -> None:
        self.lineEdit.setText(str(value))

    def get_value(self) -> str:
        return self.lineEdit.text()

    def clear(self) -> None:
        self.lineEdit.clear()


class ComboBoxConfigCard(SettingCard):
    """下拉选择配置卡片"""

    def __init__(
        self,
        icon: FluentIconBase,
        title: str,
        texts: list[str] | None = None,
        content: str | None = None,
        parent: QWidget = None,
    ) -> None:
        """初始化


        Args:
            icon (FluentIconBase): 图标
            title (str): 标题
            texts (list[str] | None, optional): 下拉选项列表. Defaults to None.
            content (str | None, optional): 内容. Defaults to None.
            parent (QWidget, optional): 父控件. Defaults to None.
        """
        super().__init__(icon, title, content, parent)
        self.texts = texts or []
        self.comboBox = ComboBox(self)
        self.comboBox.addItems(self.texts)
        self.comboBox.setFixedWidth(165)
        self.hBoxLayout.addWidget(self.comboBox, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def fill_value(self, value: str) -> None:
        self.comboBox.setCurrentText(value)

    def get_value(self) -> str:
        return self.comboBox.currentText()

    def clear(self) -> None:
        self.comboBox.setCurrentIndex(0)


class SwitchConfigCard(SettingCard):
    """开关配置卡片"""

    def __init__(
        self, icon: FluentIconBase, title: str, content: str = None, value: bool = False, parent: QWidget | None = None
    ) -> None:
        """初始化

        Args:
            icon (FluentIconBase): 图标
            title (str): 标题
            content (str, optional): 内容. Defaults to None.
            value (bool, optional): 配置值. Defaults to False.
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(icon, title, content, parent)
        self.switchButton = SwitchButton(self, IndicatorPosition.RIGHT)

        self.fill_value(value)
        self.hBoxLayout.addWidget(self.switchButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def fill_value(self, value: bool) -> None:
        self.switchButton.setChecked(value)

    def get_value(self) -> bool:
        return self.switchButton.isChecked()

    def clear(self) -> None:
        self.switchButton.setChecked(False)


class FolderConfigCard(SettingCard):
    """文件夹选择配置卡片"""

    def __init__(self, icon: FluentIconBase, title: str, content: str = None, parent: QWidget | None = None) -> None:
        """初始化

        Args:
            icon (FluentIconBase): 图标
            title (str): 标题
            content (str, optional): 内容. Defaults to None.
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(icon, title, content, parent)
        self.default = content
        self.chooseFolderButton = PushButton(icon=FluentIcon.FOLDER, text=self.tr("Choose Folder"))
        self.chooseFolderButton.clicked.connect(self.choose_folder)

        self.hBoxLayout.addWidget(self.chooseFolderButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def choose_folder(self) -> None:
        """选择文件夹

        打开文件夹选择对话框
        如果选择了文件夹,则设置内容为选择的文件夹路径,并调整卡片高度
        """
        folder = QFileDialog.getExistingDirectory(
            parent=self,
            caption=self.tr("Choose folder"),
            dir=QStandardPaths.writableLocation(QStandardPaths.StandardLocation.DesktopLocation),
        )
        if folder:
            self.setContent(folder)
            self.setFixedHeight(70)

    def fill_value(self, value: str) -> None:
        self.setContent(value)

    def get_value(self) -> str:
        return self.contentLabel.text()

    def clear(self) -> None:
        self.contentLabel.setText(self.default)


class ShowDialogCardBase(SettingCard):
    """显示对话框卡片, 只有一个按钮, 点击后显示对话框"""

    def __init__(
        self,
        dialog: MessageBoxBase,
        icon: FluentIconBase,
        title: str,
        content: str | None = None,
        parent: QWidget | None = None,
    ) -> None:
        """初始化

        Args:
            dialog (MessageBoxBase): 要显示的对话框
            icon (FluentIconBase): 图标
            title (str): 标题
            content (str | None, optional): 内容. Defaults to None.
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(icon, title, content, parent)
        self._dialog = dialog
        self.button = TransparentPushButton(FluentIcon.SETTING, self.tr("点我配置"))
        self._value = None

        self.button.clicked.connect(self.slot_button_show_dialog)

        self.hBoxLayout.addWidget(self.button, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def slot_button_show_dialog(self) -> None:
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        self._dialog(it(MainWindow)).exec()


class ShowDialogCard(ShowDialogCardBase):
    """显示对话框卡片, 通过参数返回"""

    def __init__(self, dialog, icon, title, content=None, parent=None):
        super().__init__(dialog, icon, title, content, parent)
        self._dialog_class = dialog

    # ==================== 公共方法 ====================
    def ensure_instance(self) -> None:
        """用于确保 dialog 已经实例化"""
        # 检测是否实例化, 如果没有实例化则实例化
        if not isinstance(self._dialog, self._dialog_class):
            # 项目内模块导入
            from src.ui.window.main_window import MainWindow

            self._dialog = self._dialog(it(MainWindow))

    def get_value(self) -> BaseModel:
        """获取配置"""
        self.ensure_instance()
        return self._dialog.get_config()

    def fill_value(self, config: BaseModel) -> None:
        """填充配置"""
        if config is None:
            return

        self.ensure_instance()
        self._config = config
        self._dialog.fill_config(config)

    def clear(self) -> None:
        """清空配置"""
        self.ensure_instance()
        self._dialog.clear_config()

    # ==================== 重写方法 ====================
    def slot_button_show_dialog(self) -> None:
        """重写以展示"""
        self.ensure_instance()
        self._dialog.exec()


class VersionInfoCard(SettingCard):
    """版本信息卡片"""

    def __init__(
        self,
        icon: FluentIconBase,
        title: str,
        content: str | None = None,
        version: str | None = None,
        parent: QWidget | None = None,
    ) -> None:
        """初始化

        Args:
            icon (FluentIconBase): 图标
            title (str): 标题
            content (str | None, optional): 内容. Defaults to None.
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(icon, title, content, parent)

        # 版本信息标签
        self.version_label = HyperlinkLabel(version, self)

        # 添加到布局
        self.hBoxLayout.addWidget(self.version_label, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(32)


class FontFamilyConfigCatd(SettingCard):
    """字体配置卡片"""

    def __init__(
        self,
        configItem: ConfigItem,
        icon: FluentIconBase,
        title: str,
        content: str | None = None,
        parent: QWidget | None = None,
    ) -> None:
        """初始化

        Args:
            configItem (ConfigItem): 配置项
            icon (FluentIconBase): 图标
            title (str): 标题
            content (str | None, optional): 内容. Defaults to None.
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(icon, title, content, parent)
        # 设置属性
        self._configItem = configItem

        # 字体选择下拉框
        self.font_combo_box = ComboBox(self)
        self.font_combo_box.addItems(cfg.get(cfg.fontFamilies) + self._get_system_font_families())
        self.font_combo_box.setFixedWidth(165)

        # 连接信号
        self.font_combo_box.currentTextChanged.connect(self.slot_choose_font_family)

        # 添加到布局
        self.hBoxLayout.addWidget(self.font_combo_box, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def _get_system_font_families(self) -> list[str]:
        """获取系统字体列表"""
        from PySide6.QtGui import QFontDatabase

        font_db = QFontDatabase()
        return font_db.families()

    # ==================== 槽函数 ====================
    def slot_choose_font_family(self) -> None:
        """选择字体槽函数"""
        if font_family := self.font_combo_box.currentText():
            setFontFamilies([font_family, "Segoe UI", "Microsoft YaHei", "PingFang SC"], save=True)
