# -*- coding: utf-8 -*-
"""
pytest 配置文件

此文件包含所有测试的共享 fixtures 和配置
"""
# 标准库导入
import sys
from pathlib import Path

# 第三方库导入
import pytest

# 添加项目根目录到 Python 路径
ROOT_DIR = Path(__file__).parent.parent
sys.path.insert(0, str(ROOT_DIR))


@pytest.fixture(scope="session")
def qapp():
    """
    创建 QApplication 实例
    
    scope="session" 表示整个测试会话只创建一次
    这对于 PySide6/PyQt 应用程序测试是必需的
    """
    try:
        from PySide6.QtWidgets import QApplication
        
        app = QApplication.instance()
        if app is None:
            app = QApplication(sys.argv)
        yield app
        # 不需要手动清理，pytest-qt 会处理
    except ImportError:
        pytest.skip("PySide6 not installed, skipping GUI tests")


@pytest.fixture
def mock_config_data():
    """提供模拟配置数据"""
    return {
        "qq": "123456789",
        "name": "测试机器人",
        "token": "test_token_123",
        "http": {
            "enable": True,
            "host": "127.0.0.1",
            "port": 3000,
            "secret": "test_secret",
            "enableHeart": True,
            "enablePost": False,
        },
        "ws": {
            "enable": False,
        },
        "reverseWs": {
            "enable": False,
        },
        "GroupLocalTime": {
            "RecordSendTime": False,
        },
        "debug": False,
        "log": True,
    }


@pytest.fixture
def temp_config_dir(tmp_path):
    """创建临时配置目录"""
    config_dir = tmp_path / "config"
    config_dir.mkdir()
    return config_dir
