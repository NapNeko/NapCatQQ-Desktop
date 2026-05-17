# -*- coding: utf-8 -*-
"""Unit tests for src/core/agent/model_capability_inference.py.

验证模型能力启发式推断纯函数的正确性，覆盖 Requirements 3.1-3.5 定义的关键字规则。
"""

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.agent.model_capability_inference import (
    infer_model_capabilities,
    infer_supports_embedding,
    infer_supports_rerank,
    infer_supports_reasoning,
    infer_supports_tools,
    infer_supports_vision,
)


# ===== infer_supports_vision (Requirements 3.1) =====


class TestInferSupportsVision:
    """测试 supports_vision 推断规则."""

    @pytest.mark.parametrize(
        "model_id",
        [
            "gpt-4-vision-preview",
            "gpt-4o-mini",
            "gpt-4o",
            "claude-3-opus-20240229",
            "claude-3-sonnet-20240229",
            "gemini-1.5-pro",
            "gemini-pro-vision",
            "some-model-with-VISION",
            "GPT-4O-2024",
        ],
    )
    def test_vision_positive(self, model_id: str) -> None:
        """包含 vision/4o/gpt-4o/claude-3/gemini 关键字应返回 True."""
        assert infer_supports_vision(model_id) is True

    @pytest.mark.parametrize(
        "model_id",
        [
            "claude-3-haiku-20240307",
            "Claude-3-Haiku",
            "CLAUDE-3-HAIKU-latest",
        ],
    )
    def test_vision_claude3_haiku_excluded(self, model_id: str) -> None:
        """claude-3 + haiku 同时出现应返回 False."""
        assert infer_supports_vision(model_id) is False

    @pytest.mark.parametrize(
        "model_id",
        [
            "gpt-3.5-turbo",
            "deepseek-chat",
            "qwen-turbo",
            "llama-3-70b",
        ],
    )
    def test_vision_negative(self, model_id: str) -> None:
        """不包含任何 vision 关键字应返回 False."""
        assert infer_supports_vision(model_id) is False


# ===== infer_supports_reasoning (Requirements 3.2) =====


class TestInferSupportsReasoning:
    """测试 supports_reasoning 推断规则."""

    @pytest.mark.parametrize(
        "model_id",
        [
            "o1-preview",
            "o1-mini",
            "o3-mini",
            "reasoning-model",
            "deepseek-r1",
            "qwen-think-v2",
            "model-with-thinking",
            "O1-Preview",
            "DEEPSEEK-R1",
        ],
    )
    def test_reasoning_positive(self, model_id: str) -> None:
        """包含 o1/o3(边界匹配)/reasoning/think/deepseek-r1 应返回 True."""
        assert infer_supports_reasoning(model_id) is True

    @pytest.mark.parametrize(
        "model_id",
        [
            # o1 字符串起始位置匹配
            "o1",
            "o3",
            # o1/o3 连字符为左边界
            "prefix-o1",
            "prefix-o3-suffix",
        ],
    )
    def test_reasoning_boundary_match(self, model_id: str) -> None:
        """o1/o3 以字符串起始或连字符为左边界时应匹配."""
        assert infer_supports_reasoning(model_id) is True

    @pytest.mark.parametrize(
        "model_id",
        [
            "pro1-model",
            "modelo1",
            "gpt-4o",
            "audio3-model",
            "polo3x",
        ],
    )
    def test_reasoning_no_boundary_match(self, model_id: str) -> None:
        """o1/o3 无左边界（非连字符/非字符串起始）不应匹配."""
        assert infer_supports_reasoning(model_id) is False

    @pytest.mark.parametrize(
        "model_id",
        [
            "gpt-3.5-turbo",
            "claude-3-opus",
            "deepseek-chat",
        ],
    )
    def test_reasoning_negative(self, model_id: str) -> None:
        """不包含任何 reasoning 关键字应返回 False."""
        assert infer_supports_reasoning(model_id) is False


# ===== infer_supports_tools (Requirements 3.3) =====


class TestInferSupportsTools:
    """测试 supports_tools 推断规则."""

    @pytest.mark.parametrize(
        "model_id",
        [
            "gpt-4o",
            "claude-3-opus",
            "deepseek-chat",
            "qwen-turbo",
            "llama-3-70b",
        ],
    )
    def test_tools_default_true(self, model_id: str) -> None:
        """不包含排除关键字时默认返回 True."""
        assert infer_supports_tools(model_id) is True

    @pytest.mark.parametrize(
        "model_id",
        [
            "text-embedding-ada-002",
            "text-embedding-3-small",
            "bge-rerank-v2",
            "tts-1",
            "tts-1-hd",
            "whisper-1",
            "dall-e-3",
            "DALL-E-2",
            "Embedding-Model",
        ],
    )
    def test_tools_excluded(self, model_id: str) -> None:
        """包含 embedding/rerank/tts/whisper/dall-e 应返回 False."""
        assert infer_supports_tools(model_id) is False


# ===== infer_supports_embedding (Requirements 3.4) =====


class TestInferSupportsEmbedding:
    """测试 supports_embedding 推断规则."""

    @pytest.mark.parametrize(
        "model_id",
        [
            "text-embedding-ada-002",
            "text-embedding-3-small",
            "Embedding-v2",
            "bge-embed-base",
            "nomic-embed-text",
        ],
    )
    def test_embedding_positive(self, model_id: str) -> None:
        """包含 embedding/embed 应返回 True."""
        assert infer_supports_embedding(model_id) is True

    @pytest.mark.parametrize(
        "model_id",
        [
            "gpt-4o",
            "claude-3-opus",
            "deepseek-chat",
        ],
    )
    def test_embedding_negative(self, model_id: str) -> None:
        """不包含 embedding/embed 应返回 False."""
        assert infer_supports_embedding(model_id) is False


# ===== infer_supports_rerank (Requirements 3.5) =====


class TestInferSupportsRerank:
    """测试 supports_rerank 推断规则."""

    @pytest.mark.parametrize(
        "model_id",
        [
            "bge-rerank-v2",
            "cohere-rerank-english-v3",
            "RERANK-model",
        ],
    )
    def test_rerank_positive(self, model_id: str) -> None:
        """包含 rerank 应返回 True."""
        assert infer_supports_rerank(model_id) is True

    @pytest.mark.parametrize(
        "model_id",
        [
            "gpt-4o",
            "text-embedding-ada-002",
            "deepseek-chat",
        ],
    )
    def test_rerank_negative(self, model_id: str) -> None:
        """不包含 rerank 应返回 False."""
        assert infer_supports_rerank(model_id) is False


# ===== infer_model_capabilities (集成) =====


class TestInferModelCapabilities:
    """测试 infer_model_capabilities 主函数返回结构正确."""

    def test_returns_all_keys(self) -> None:
        """返回字典应包含所有 5 个能力键."""
        result = infer_model_capabilities("gpt-4o")
        expected_keys = {
            "supports_vision",
            "supports_reasoning",
            "supports_tools",
            "supports_embedding",
            "supports_rerank",
        }
        assert set(result.keys()) == expected_keys

    def test_all_values_are_bool(self) -> None:
        """返回字典的所有值应为 bool 类型."""
        result = infer_model_capabilities("some-random-model")
        for value in result.values():
            assert isinstance(value, bool)

    def test_gpt4o_capabilities(self) -> None:
        """gpt-4o 应支持 vision 和 tools, 不支持其他."""
        result = infer_model_capabilities("gpt-4o")
        assert result["supports_vision"] is True
        assert result["supports_reasoning"] is False
        assert result["supports_tools"] is True
        assert result["supports_embedding"] is False
        assert result["supports_rerank"] is False

    def test_embedding_model_capabilities(self) -> None:
        """embedding 模型应支持 embedding, 不支持 tools."""
        result = infer_model_capabilities("text-embedding-3-small")
        assert result["supports_vision"] is False
        assert result["supports_reasoning"] is False
        assert result["supports_tools"] is False
        assert result["supports_embedding"] is True
        assert result["supports_rerank"] is False

    def test_rerank_model_capabilities(self) -> None:
        """rerank 模型应支持 rerank, 不支持 tools."""
        result = infer_model_capabilities("bge-rerank-v2")
        assert result["supports_vision"] is False
        assert result["supports_reasoning"] is False
        assert result["supports_tools"] is False
        assert result["supports_embedding"] is False
        assert result["supports_rerank"] is True

    def test_reasoning_model_capabilities(self) -> None:
        """o1-preview 应支持 reasoning 和 tools."""
        result = infer_model_capabilities("o1-preview")
        assert result["supports_vision"] is False
        assert result["supports_reasoning"] is True
        assert result["supports_tools"] is True
        assert result["supports_embedding"] is False
        assert result["supports_rerank"] is False
