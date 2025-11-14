# -*- coding: utf-8 -*-
"""测试路径功能模块"""
# 标准库导入
from pathlib import Path
from unittest.mock import patch

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.utils.path_func import PathFunc


class TestPathFunc:
    """测试 PathFunc 类"""

    def test_singleton_pattern(self):
        """测试单例模式"""
        instance1 = PathFunc()
        instance2 = PathFunc()
        assert instance1 is instance2

    def test_path_initialization(self):
        """测试路径初始化"""
        path_func = PathFunc()
        
        # 检查基础路径
        assert path_func.base_path == Path.cwd()
        assert path_func.runtime_path == Path.cwd() / "runtime"
        
        # 检查运行时路径
        assert path_func.napcat_path == Path.cwd() / "runtime" / "NapCatQQ"
        assert path_func.config_dir_path == Path.cwd() / "runtime" / "config"
        assert path_func.tmp_path == Path.cwd() / "runtime" / "tmp"
        
        # 检查文件路径
        assert path_func.config_path == Path.cwd() / "runtime" / "config" / "config.json"
        assert path_func.bot_config_path == Path.cwd() / "runtime" / "config" / "bot.json"

    def test_path_validator_creates_directories(self, tmp_path, monkeypatch):
        """测试路径验证器创建目录"""
        # 更改工作目录到临时路径
        monkeypatch.chdir(tmp_path)
        
        # 创建新实例（因为单例，需要确保测试隔离）
        # 先清理单例
        if hasattr(PathFunc, '_instances'):
            PathFunc._instances.clear()
        
        path_func = PathFunc()
        path_func.path_validator()
        
        # 验证目录已创建
        assert path_func.tmp_path.exists()
        assert path_func.config_dir_path.exists()
        assert path_func.napcat_path.exists()

    def test_old_version_path_v1613(self):
        """测试旧版本路径 v1.6.13"""
        old_paths = PathFunc.OldVersionPath._v1613()
        
        assert "napcat_path" in old_paths
        assert "config_dir_path" in old_paths
        assert "tmp_path" in old_paths
        
        assert old_paths["napcat_path"] == Path.cwd() / "NapCat"
        assert old_paths["config_dir_path"] == Path.cwd() / "config"
        assert old_paths["tmp_path"] == Path.cwd() / "tmp"


@pytest.mark.unit
class TestPathFuncUnit:
    """PathFunc 单元测试（标记为 unit）"""

    def test_path_attributes_are_path_objects(self):
        """测试路径属性都是 Path 对象"""
        path_func = PathFunc()
        
        assert isinstance(path_func.base_path, Path)
        assert isinstance(path_func.runtime_path, Path)
        assert isinstance(path_func.napcat_path, Path)
        assert isinstance(path_func.config_dir_path, Path)
        assert isinstance(path_func.tmp_path, Path)
        assert isinstance(path_func.config_path, Path)
        assert isinstance(path_func.bot_config_path, Path)
