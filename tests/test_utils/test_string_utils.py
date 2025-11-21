# -*- coding: utf-8 -*-
"""测试字符串和数据处理工具"""
# 标准库导入
import json
import re


class TestStringUtils:
    """测试字符串工具"""

    def test_string_strip(self):
        """测试字符串去除空白"""
        text = "  hello world  "
        stripped = text.strip()
        
        assert stripped == "hello world"
        assert not stripped.startswith(" ")
        assert not stripped.endswith(" ")

    def test_string_format(self):
        """测试字符串格式化"""
        name = "TestBot"
        qq_id = "123456"
        formatted = f"Bot: {name}, QQ: {qq_id}"
        
        assert name in formatted
        assert qq_id in formatted

    def test_multiline_string_join(self):
        """测试多行字符串连接"""
        lines = ["Line 1", "Line 2", "Line 3"]
        joined = "\n".join(lines)
        
        assert "Line 1" in joined
        assert "Line 2" in joined
        assert joined.count("\n") == 2

    def test_string_replace(self):
        """测试字符串替换"""
        text = "Hello World"
        replaced = text.replace("World", "Python")
        
        assert replaced == "Hello Python"
        assert "World" not in replaced


class TestRegexPatterns:
    """测试正则表达式模式"""

    def test_qq_number_pattern(self):
        """测试 QQ 号码格式"""
        qq_pattern = r"^\d{5,11}$"
        
        assert re.match(qq_pattern, "123456")
        assert re.match(qq_pattern, "12345678901")
        assert not re.match(qq_pattern, "abc123")
        assert not re.match(qq_pattern, "1234")  # 太短

    def test_email_pattern(self):
        """测试邮箱格式"""
        email_pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
        
        assert re.match(email_pattern, "test@example.com")
        assert re.match(email_pattern, "user.name@example.co.uk")
        assert not re.match(email_pattern, "invalid.email")

    def test_url_pattern(self):
        """测试 URL 格式"""
        url_pattern = r"^https?://[^\s]+$"
        
        assert re.match(url_pattern, "https://example.com")
        assert re.match(url_pattern, "http://example.com/path")
        assert not re.match(url_pattern, "ftp://example.com")


class TestDataValidation:
    """测试数据验证"""

    def test_port_range_validation(self):
        """测试端口范围验证"""
        def is_valid_port(port):
            return isinstance(port, int) and 1 <= port <= 65535
        
        assert is_valid_port(80)
        assert is_valid_port(3000)
        assert is_valid_port(65535)
        assert not is_valid_port(0)
        assert not is_valid_port(65536)
        assert not is_valid_port("3000")

    def test_empty_string_validation(self):
        """测试空字符串验证"""
        def is_not_empty(text):
            return bool(text and text.strip())
        
        assert is_not_empty("hello")
        assert not is_not_empty("")
        assert not is_not_empty("   ")
        assert not is_not_empty(None)

    def test_list_not_empty(self):
        """测试列表非空验证"""
        assert len([1, 2, 3]) > 0
        assert not (len([]) > 0)


class TestJSONHandling:
    """测试 JSON 处理"""

    def test_json_encode(self):
        """测试 JSON 编码"""
        data = {"name": "test", "value": 123}
        json_str = json.dumps(data)
        
        assert isinstance(json_str, str)
        assert "name" in json_str
        assert "test" in json_str

    def test_json_decode(self):
        """测试 JSON 解码"""
        json_str = '{"name": "test", "value": 123}'
        data = json.loads(json_str)
        
        assert isinstance(data, dict)
        assert data["name"] == "test"
        assert data["value"] == 123

    def test_json_pretty_print(self):
        """测试 JSON 美化打印"""
        data = {"key": "value"}
        pretty = json.dumps(data, indent=2, ensure_ascii=False)
        
        assert "\n" in pretty
        assert "  " in pretty

    def test_chinese_in_json(self):
        """测试 JSON 中的中文"""
        data = {"名称": "测试"}
        json_str = json.dumps(data, ensure_ascii=False)
        
        assert "测试" in json_str
