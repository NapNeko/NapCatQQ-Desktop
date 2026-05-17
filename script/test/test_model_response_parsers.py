# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/model_response_parsers.py.

验证 OpenAI、Anthropic、Gemini、Azure 四种响应格式的解析纯函数正确性。
"""

import pytest

from src.core.agent.model_response_parsers import (
    parse_anthropic_model_list,
    parse_azure_model_list,
    parse_gemini_model_list,
    parse_openai_model_list,
)


# ============================================================
# OpenAI 格式解析测试
# ============================================================


class TestParseOpenaiModelList:
    """测试 parse_openai_model_list 函数."""

    def test_valid_response(self):
        """正常响应应提取所有 id."""
        data = {
            "data": [
                {"id": "gpt-4", "object": "model"},
                {"id": "gpt-3.5-turbo", "object": "model"},
            ]
        }
        assert parse_openai_model_list(data) == ["gpt-4", "gpt-3.5-turbo"]

    def test_missing_data_field(self):
        """缺少 data 字段应返回空列表."""
        assert parse_openai_model_list({}) == []
        assert parse_openai_model_list({"models": []}) == []

    def test_data_not_list(self):
        """data 字段非列表应返回空列表."""
        assert parse_openai_model_list({"data": "not a list"}) == []
        assert parse_openai_model_list({"data": None}) == []
        assert parse_openai_model_list({"data": 123}) == []

    def test_skip_missing_id(self):
        """缺少 id 字段的对象应被跳过."""
        data = {"data": [{"object": "model"}, {"id": "gpt-4"}]}
        assert parse_openai_model_list(data) == ["gpt-4"]

    def test_skip_null_id(self):
        """id 为 None 的对象应被跳过."""
        data = {"data": [{"id": None}, {"id": "gpt-4"}]}
        assert parse_openai_model_list(data) == ["gpt-4"]

    def test_skip_empty_string_id(self):
        """id 为空字符串的对象应被跳过."""
        data = {"data": [{"id": ""}, {"id": "gpt-4"}]}
        assert parse_openai_model_list(data) == ["gpt-4"]

    def test_skip_non_string_id(self):
        """id 为非字符串类型的对象应被跳过."""
        data = {"data": [{"id": 123}, {"id": True}, {"id": ["a"]}, {"id": "gpt-4"}]}
        assert parse_openai_model_list(data) == ["gpt-4"]

    def test_skip_non_dict_items(self):
        """data 数组中的非字典元素应被跳过."""
        data = {"data": ["string", 123, None, {"id": "gpt-4"}]}
        assert parse_openai_model_list(data) == ["gpt-4"]

    def test_empty_data_array(self):
        """空 data 数组应返回空列表."""
        assert parse_openai_model_list({"data": []}) == []


# ============================================================
# Anthropic 格式解析测试
# ============================================================


class TestParseAnthropicModelList:
    """测试 parse_anthropic_model_list 函数."""

    def test_valid_response(self):
        """正常响应应提取所有 id."""
        data = {
            "data": [
                {"id": "claude-3-opus-20240229", "type": "model"},
                {"id": "claude-3-sonnet-20240229", "type": "model"},
            ],
            "has_more": False,
        }
        assert parse_anthropic_model_list(data) == [
            "claude-3-opus-20240229",
            "claude-3-sonnet-20240229",
        ]

    def test_missing_data_field(self):
        """缺少 data 字段应返回空列表."""
        assert parse_anthropic_model_list({"has_more": False}) == []

    def test_skip_invalid_entries(self):
        """无效条目应被跳过."""
        data = {
            "data": [
                {"id": None},
                {"id": ""},
                {"id": 42},
                {"type": "model"},
                {"id": "claude-3-opus-20240229"},
            ]
        }
        assert parse_anthropic_model_list(data) == ["claude-3-opus-20240229"]


# ============================================================
# Gemini 格式解析测试
# ============================================================


class TestParseGeminiModelList:
    """测试 parse_gemini_model_list 函数."""

    def test_valid_response_with_prefix(self):
        """带 models/ 前缀的 name 应去除前缀."""
        data = {
            "models": [
                {"name": "models/gemini-pro", "displayName": "Gemini Pro"},
                {"name": "models/gemini-pro-vision", "displayName": "Gemini Pro Vision"},
            ]
        }
        assert parse_gemini_model_list(data) == ["gemini-pro", "gemini-pro-vision"]

    def test_valid_response_without_prefix(self):
        """不带 models/ 前缀的 name 应直接使用."""
        data = {
            "models": [
                {"name": "gemini-pro"},
                {"name": "custom-model"},
            ]
        }
        assert parse_gemini_model_list(data) == ["gemini-pro", "custom-model"]

    def test_mixed_prefix(self):
        """混合有无前缀的情况."""
        data = {
            "models": [
                {"name": "models/gemini-pro"},
                {"name": "custom-model"},
            ]
        }
        assert parse_gemini_model_list(data) == ["gemini-pro", "custom-model"]

    def test_missing_models_field(self):
        """缺少 models 字段应返回空列表."""
        assert parse_gemini_model_list({}) == []
        assert parse_gemini_model_list({"data": []}) == []

    def test_models_not_list(self):
        """models 字段非列表应返回空列表."""
        assert parse_gemini_model_list({"models": "not a list"}) == []
        assert parse_gemini_model_list({"models": None}) == []

    def test_skip_missing_name(self):
        """缺少 name 字段的对象应被跳过."""
        data = {"models": [{"displayName": "Test"}, {"name": "models/gemini-pro"}]}
        assert parse_gemini_model_list(data) == ["gemini-pro"]

    def test_skip_null_name(self):
        """name 为 None 的对象应被跳过."""
        data = {"models": [{"name": None}, {"name": "models/gemini-pro"}]}
        assert parse_gemini_model_list(data) == ["gemini-pro"]

    def test_skip_empty_string_name(self):
        """name 为空字符串的对象应被跳过."""
        data = {"models": [{"name": ""}, {"name": "models/gemini-pro"}]}
        assert parse_gemini_model_list(data) == ["gemini-pro"]

    def test_skip_non_string_name(self):
        """name 为非字符串类型的对象应被跳过."""
        data = {"models": [{"name": 123}, {"name": "models/gemini-pro"}]}
        assert parse_gemini_model_list(data) == ["gemini-pro"]

    def test_prefix_only_name_skipped(self):
        """name 仅为 "models/" 前缀（去除后为空）应被跳过."""
        data = {"models": [{"name": "models/"}, {"name": "models/gemini-pro"}]}
        assert parse_gemini_model_list(data) == ["gemini-pro"]

    def test_empty_models_array(self):
        """空 models 数组应返回空列表."""
        assert parse_gemini_model_list({"models": []}) == []

    def test_skip_non_dict_items(self):
        """models 数组中的非字典元素应被跳过."""
        data = {"models": ["string", None, {"name": "models/gemini-pro"}]}
        assert parse_gemini_model_list(data) == ["gemini-pro"]


# ============================================================
# Azure 格式解析测试
# ============================================================


class TestParseAzureModelList:
    """测试 parse_azure_model_list 函数."""

    def test_valid_response(self):
        """正常响应应提取所有 id."""
        data = {
            "data": [
                {"id": "gpt-4", "status": "succeeded"},
                {"id": "gpt-35-turbo", "status": "succeeded"},
            ]
        }
        assert parse_azure_model_list(data) == ["gpt-4", "gpt-35-turbo"]

    def test_missing_data_field(self):
        """缺少 data 字段应返回空列表."""
        assert parse_azure_model_list({}) == []

    def test_skip_invalid_entries(self):
        """无效条目应被跳过."""
        data = {
            "data": [
                {"id": None},
                {"id": ""},
                {"id": 42},
                {"status": "succeeded"},
                {"id": "gpt-4"},
            ]
        }
        assert parse_azure_model_list(data) == ["gpt-4"]
