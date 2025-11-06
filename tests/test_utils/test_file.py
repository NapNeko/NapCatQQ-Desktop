# -*- coding: utf-8 -*-
"""测试文件工具类"""
# 标准库导入
from pathlib import Path

# 第三方库导入
import pytest
from PySide6.QtCore import QFile

# 项目内模块导入
from src.core.utils.file import QFluentFile


class TestQFluentFile:
    """测试 QFluentFile 上下文管理器"""

    def test_read_file_with_context_manager(self, tmp_path):
        """测试使用上下文管理器读取文件"""
        # 创建测试文件
        test_file = tmp_path / "test.txt"
        test_content = "Hello, NapCatQQ!"
        test_file.write_text(test_content, encoding="utf-8")

        # 使用 QFluentFile 读取
        with QFluentFile(test_file, QFile.OpenModeFlag.ReadOnly) as file:
            content = file.readAll().data().decode("utf-8")
            assert content == test_content

    def test_file_closes_after_context(self, tmp_path):
        """测试退出上下文后文件是否关闭"""
        test_file = tmp_path / "test.txt"
        test_file.write_text("test", encoding="utf-8")

        file = QFluentFile(test_file, QFile.OpenModeFlag.ReadOnly)
        with file:
            assert file.isOpen()
        
        # 退出上下文后应该关闭
        assert not file.isOpen()

    def test_read_nonexistent_file_raises_error(self, tmp_path):
        """测试读取不存在的文件抛出错误"""
        nonexistent_file = tmp_path / "nonexistent.txt"
        
        with pytest.raises(IOError):
            with QFluentFile(nonexistent_file, QFile.OpenModeFlag.ReadOnly):
                pass

    def test_accepts_string_path(self, tmp_path):
        """测试接受字符串路径"""
        test_file = tmp_path / "test.txt"
        test_file.write_text("test", encoding="utf-8")

        with QFluentFile(str(test_file), QFile.OpenModeFlag.ReadOnly) as file:
            content = file.readAll().data().decode("utf-8")
            assert content == "test"

    def test_accepts_path_object(self, tmp_path):
        """测试接受 Path 对象"""
        test_file = tmp_path / "test.txt"
        test_file.write_text("test", encoding="utf-8")

        with QFluentFile(test_file, QFile.OpenModeFlag.ReadOnly) as file:
            content = file.readAll().data().decode("utf-8")
            assert content == "test"
