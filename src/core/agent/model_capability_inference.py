# -*- coding: utf-8 -*-
"""模型能力启发式推断纯函数.

根据 model_id 中的关键字，通过大小写不敏感匹配推断模型的各项能力标志。
所有函数均为纯函数，无副作用，便于属性测试验证正确性。
"""

from __future__ import annotations

import re


def infer_model_capabilities(model_id: str) -> dict[str, bool]:
    """根据 model_id 关键字启发式推断模型能力.

    对 model_id 执行大小写不敏感的子串/边界匹配，返回各能力标志的推断结果。

    Args:
        model_id: 模型标识符字符串。

    Returns:
        包含以下键的字典:
        - supports_vision: 是否支持视觉输入
        - supports_reasoning: 是否支持推理
        - supports_tools: 是否支持工具调用
        - supports_embedding: 是否支持嵌入
        - supports_rerank: 是否支持重排序
    """
    return {
        "supports_vision": infer_supports_vision(model_id),
        "supports_reasoning": infer_supports_reasoning(model_id),
        "supports_tools": infer_supports_tools(model_id),
        "supports_embedding": infer_supports_embedding(model_id),
        "supports_rerank": infer_supports_rerank(model_id),
    }


def infer_supports_vision(model_id: str) -> bool:
    """推断 supports_vision 能力.

    规则 (Requirements 3.1):
    - 模型 ID 包含 "vision"/"4o"/"gpt-4o"/"claude-3"/"gemini" 中任一关键字时为 True
    - 但模型 ID 同时包含 "claude-3" 和 "haiku" 时为 False

    所有匹配均为大小写不敏感。

    Args:
        model_id: 模型标识符字符串。

    Returns:
        是否支持视觉输入。
    """
    lower_id = model_id.lower()

    # 特殊排除规则: claude-3 + haiku → False
    if "claude-3" in lower_id and "haiku" in lower_id:
        return False

    # 正向关键字匹配
    vision_keywords = ("vision", "4o", "gpt-4o", "claude-3", "gemini")
    return any(kw in lower_id for kw in vision_keywords)


def infer_supports_reasoning(model_id: str) -> bool:
    """推断 supports_reasoning 能力.

    规则 (Requirements 3.2):
    - 模型 ID 包含 "reasoning"/"think"/"deepseek-r1" 时为 True
    - 模型 ID 包含 "o1"/"o3" 时为 True，但须以连字符或字符串起始位置为左边界
      (即匹配 "o1-"、"-o1" 但不匹配 "pro1")

    所有匹配均为大小写不敏感。

    Args:
        model_id: 模型标识符字符串。

    Returns:
        是否支持推理。
    """
    lower_id = model_id.lower()

    # 简单子串关键字匹配
    simple_keywords = ("reasoning", "think", "deepseek-r1")
    if any(kw in lower_id for kw in simple_keywords):
        return True

    # o1/o3 边界匹配: 左边界为连字符或字符串起始位置
    # 使用正则: (?:^|-)o[13] 匹配字符串开头或连字符后紧跟 o1 或 o3
    if re.search(r"(?:^|-)o[13]", lower_id):
        return True

    return False


def infer_supports_tools(model_id: str) -> bool:
    """推断 supports_tools 能力.

    规则 (Requirements 3.3):
    - 默认为 True
    - 模型 ID 包含 "embedding"/"rerank"/"tts"/"whisper"/"dall-e" 中任一关键字时为 False

    所有匹配均为大小写不敏感。

    Args:
        model_id: 模型标识符字符串。

    Returns:
        是否支持工具调用。
    """
    lower_id = model_id.lower()

    # 排除关键字: 包含任一则不支持工具调用
    exclude_keywords = ("embedding", "rerank", "tts", "whisper", "dall-e")
    if any(kw in lower_id for kw in exclude_keywords):
        return False

    return True


def infer_supports_embedding(model_id: str) -> bool:
    """推断 supports_embedding 能力.

    规则 (Requirements 3.4):
    - 模型 ID 包含 "embedding"/"embed" 中任一关键字时为 True，否则为 False

    所有匹配均为大小写不敏感。

    Args:
        model_id: 模型标识符字符串。

    Returns:
        是否支持嵌入。
    """
    lower_id = model_id.lower()
    return "embedding" in lower_id or "embed" in lower_id


def infer_supports_rerank(model_id: str) -> bool:
    """推断 supports_rerank 能力.

    规则 (Requirements 3.5):
    - 模型 ID 包含 "rerank" 关键字时为 True，否则为 False

    所有匹配均为大小写不敏感。

    Args:
        model_id: 模型标识符字符串。

    Returns:
        是否支持重排序。
    """
    lower_id = model_id.lower()
    return "rerank" in lower_id
