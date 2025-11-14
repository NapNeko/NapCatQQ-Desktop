# -*- coding: utf-8 -*-
"""测试 UI 组件基础功能"""
# 标准库导入
from unittest.mock import MagicMock

# 第三方库导入
import pytest
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.ui.components.message_box import AskBox, TextInputBox


@pytest.mark.gui
class TestTextInputBox:
    """测试文本输入框组件"""

    def test_text_input_box_creation(self, qapp, qtbot):
        """测试文本输入框创建"""
        # 创建一个模拟的父窗口
        parent = QWidget()
        qtbot.addWidget(parent)
        
        # 创建文本输入框
        input_box = TextInputBox(parent, placeholder_text="请输入QQ号")
        qtbot.addWidget(input_box)
        
        # 验证组件存在
        assert input_box.title_label is not None
        assert input_box.input_line_edit is not None
        
        # 验证占位文本
        assert input_box.input_line_edit.placeholderText() == "请输入QQ号"

    def test_text_input_box_with_default_placeholder(self, qapp, qtbot):
        """测试默认占位文本"""
        parent = QWidget()
        qtbot.addWidget(parent)
        
        input_box = TextInputBox(parent)
        qtbot.addWidget(input_box)
        
        assert input_box.input_line_edit.placeholderText() == "Enter..."

    def test_text_input_box_clear_button_enabled(self, qapp, qtbot):
        """测试清除按钮已启用"""
        parent = QWidget()
        qtbot.addWidget(parent)
        
        input_box = TextInputBox(parent)
        qtbot.addWidget(input_box)
        
        assert input_box.input_line_edit.isClearButtonEnabled()

    def test_text_input_box_text_entry(self, qapp, qtbot):
        """测试文本输入"""
        parent = QWidget()
        qtbot.addWidget(parent)
        
        input_box = TextInputBox(parent)
        qtbot.addWidget(input_box)
        
        # 模拟用户输入
        qtbot.keyClicks(input_box.input_line_edit, "123456789")
        
        # 验证输入的文本
        assert input_box.input_line_edit.text() == "123456789"


@pytest.mark.gui
class TestAskBox:
    """测试确认提示框组件"""

    def test_ask_box_creation(self, qapp, qtbot):
        """测试确认提示框创建"""
        parent = QWidget()
        qtbot.addWidget(parent)
        
        ask_box = AskBox(
            title="确认删除",
            content="确定要删除此机器人配置吗？",
            parent=parent
        )
        qtbot.addWidget(ask_box)
        
        # 验证组件存在
        assert ask_box.title_label is not None
        assert ask_box.content_label is not None
        
        # 验证标题和内容
        assert ask_box.title_label.text() == "确认删除"
        assert ask_box.content_label.text() == "确定要删除此机器人配置吗？"

    def test_ask_box_minimum_size(self, qapp, qtbot):
        """测试最小尺寸设置"""
        parent = QWidget()
        qtbot.addWidget(parent)
        
        ask_box = AskBox(
            title="测试标题",
            content="测试内容",
            parent=parent
        )
        qtbot.addWidget(ask_box)
        
        # 验证最小尺寸
        min_size = ask_box.widget.minimumSize()
        assert min_size.width() == 420
        assert min_size.height() == 230
