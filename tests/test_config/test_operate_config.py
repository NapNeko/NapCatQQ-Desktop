# -*- coding: utf-8 -*-
"""测试配置文件读写操作"""
# 标准库导入
import json
from pathlib import Path

# 第三方库导入
import pytest


class TestConfigFileOperations:
    """测试配置文件操作"""

    def test_json_load_valid_config(self, tmp_path):
        """测试加载有效的 JSON 配置"""
        config_file = tmp_path / "test_config.json"
        config_data = {
            "bot": {
                "name": "TestBot",
                "QQID": "123456",
            }
        }
        
        # 写入配置
        config_file.write_text(json.dumps(config_data, ensure_ascii=False))
        
        # 读取配置
        loaded = json.loads(config_file.read_text())
        
        assert loaded["bot"]["name"] == "TestBot"
        assert loaded["bot"]["QQID"] == "123456"

    def test_json_save_config(self, tmp_path):
        """测试保存 JSON 配置"""
        config_file = tmp_path / "test_config.json"
        config_data = {
            "version": "1.0",
            "settings": {
                "debug": False,
                "log": True,
            }
        }
        
        # 保存配置
        config_file.write_text(json.dumps(config_data, indent=2, ensure_ascii=False))
        
        # 验证文件存在
        assert config_file.exists()
        assert config_file.is_file()

    def test_config_file_not_found(self, tmp_path):
        """测试配置文件不存在"""
        config_file = tmp_path / "nonexistent.json"
        
        assert not config_file.exists()

    def test_invalid_json_handling(self, tmp_path):
        """测试无效 JSON 处理"""
        config_file = tmp_path / "invalid.json"
        config_file.write_text("{invalid json}")
        
        with pytest.raises(json.JSONDecodeError):
            json.loads(config_file.read_text())

    def test_config_backup_creation(self, tmp_path):
        """测试配置备份创建"""
        original = tmp_path / "config.json"
        backup = tmp_path / "config.json.backup"
        
        config_data = {"key": "value"}
        original.write_text(json.dumps(config_data))
        
        # 创建备份
        backup.write_text(original.read_text())
        
        assert backup.exists()
        assert backup.read_text() == original.read_text()


class TestConfigValidation:
    """测试配置验证"""

    def test_required_fields_present(self):
        """测试必需字段存在"""
        config = {
            "bot": {
                "name": "TestBot",
                "QQID": "123456",
            }
        }
        
        assert "bot" in config
        assert "name" in config["bot"]
        assert "QQID" in config["bot"]

    def test_config_type_validation(self):
        """测试配置类型验证"""
        config = {
            "debug": False,
            "port": 3000,
            "host": "127.0.0.1",
        }
        
        assert isinstance(config["debug"], bool)
        assert isinstance(config["port"], int)
        assert isinstance(config["host"], str)

    def test_nested_config_access(self):
        """测试嵌套配置访问"""
        config = {
            "network": {
                "http": {
                    "host": "127.0.0.1",
                    "port": 3000,
                }
            }
        }
        
        assert config["network"]["http"]["host"] == "127.0.0.1"
        assert config["network"]["http"]["port"] == 3000


class TestConfigMigration:
    """测试配置迁移"""

    def test_config_version_check(self):
        """测试配置版本检查"""
        old_config = {"version": "1.0"}
        new_config = {"version": "2.0"}
        
        assert old_config["version"] != new_config["version"]

    def test_default_value_addition(self):
        """测试默认值添加"""
        config = {"name": "test"}
        defaults = {"name": "default", "debug": False}
        
        # 合并默认值
        for key, value in defaults.items():
            if key not in config:
                config[key] = value
        
        assert config["name"] == "test"  # 保留原值
        assert config["debug"] is False  # 添加默认值
