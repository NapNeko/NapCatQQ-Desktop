# -*- coding: utf-8 -*-
"""模型参数钳制与消息内容转换纯函数.

提供参数值范围钳制（temperature、top_p、max_tokens）和消息内容格式转换
（数组格式 → 纯文本字符串）的纯函数实现。所有函数无副作用，便于属性测试验证。
"""

from __future__ import annotations


def clamp_temperature(value: float) -> float:
    """将 temperature 值钳制到 [0.0, 2.0] 范围.

    规则 (Requirements 5.6):
    - 小于 0.0 时返回 0.0
    - 大于 2.0 时返回 2.0
    - 在范围内时返回原值

    Args:
        value: 用户输入的 temperature 值。

    Returns:
        钳制后的 temperature 值，保证在 [0.0, 2.0] 范围内。
    """
    return max(0.0, min(value, 2.0))


def clamp_top_p(value: float) -> float:
    """将 top_p 值钳制到 [0.0, 1.0] 范围.

    规则 (Requirements 5.7):
    - 小于 0.0 时返回 0.0
    - 大于 1.0 时返回 1.0
    - 在范围内时返回原值

    Args:
        value: 用户输入的 top_p 值。

    Returns:
        钳制后的 top_p 值，保证在 [0.0, 1.0] 范围内。
    """
    return max(0.0, min(value, 1.0))


def clamp_max_tokens(value: int, max_limit: int) -> int:
    """将 max_tokens 值钳制到 [1, max_limit] 范围.

    规则 (Requirements 5.8):
    - 小于 1 时返回 1
    - 大于 max_limit 时返回 max_limit
    - 在范围内时返回原值

    Args:
        value: 用户输入的 max_tokens 值。
        max_limit: 当前模型允许的最大 token 数（ModelEntry.max_tokens）。

    Returns:
        钳制后的 max_tokens 值，保证在 [1, max_limit] 范围内。
    """
    return max(1, min(value, max_limit))


def flatten_array_content(content: list[dict]) -> str:
    """将数组格式消息内容转换为纯文本字符串.

    规则 (Requirements 4.8, 6.2):
    - 遍历 content 列表中的每个字典
    - 提取所有 type == "text" 的块的 "text" 字段值
    - 按原始顺序以换行符 ("\\n") 连接所有提取的文本
    - 丢弃 type == "image_url" 或其他非 "text" 类型的块
    - 若无 text 块，返回空字符串

    Args:
        content: 消息内容数组，每个元素为包含 "type" 字段的字典。
            示例: [{"type": "text", "text": "Hello"},
                   {"type": "image_url", "image_url": {"url": "..."}},
                   {"type": "text", "text": "World"}]

    Returns:
        所有 text 块内容按原序以换行符连接的字符串。
        示例: "Hello\\nWorld"
    """
    text_parts: list[str] = []
    for block in content:
        if block.get("type") == "text":
            text_value = block.get("text", "")
            text_parts.append(text_value)
    return "\n".join(text_parts)
