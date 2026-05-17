# -*- coding: utf-8 -*-
"""模型列表响应解析纯函数.

针对不同供应商 API 返回的 JSON 格式，提取有效的 model_id 列表。
支持 OpenAI、Anthropic、Gemini、Azure 四种响应格式。

规则:
- OpenAI: 从 "data" 数组中提取 "id" 字段
- Anthropic: 从 "data" 数组中提取 "id" 字段
- Gemini: 从 "models" 数组中提取 "name" 字段，去除 "models/" 前缀
- Azure: 从 "data" 数组中提取 "id" 字段
- 若预期数组字段缺失则返回空列表
- 跳过缺少目标字段、值为 null/空字符串/非字符串类型的对象
"""

from __future__ import annotations


def _extract_ids_from_data(data: dict) -> list[str]:
    """从响应体的 "data" 数组中提取有效的 "id" 字段值.

    Args:
        data: API 响应体字典.

    Returns:
        有效 model_id 字符串列表.
    """
    items = data.get("data")
    if not isinstance(items, list):
        return []

    result: list[str] = []
    for item in items:
        if not isinstance(item, dict):
            continue
        model_id = item.get("id")
        if isinstance(model_id, str) and model_id != "":
            result.append(model_id)
    return result


def parse_openai_model_list(data: dict) -> list[str]:
    """解析 OpenAI 格式的模型列表响应.

    从 JSON 响应体的 "data" 数组中提取每个对象的 "id" 字段作为 model_id。

    Args:
        data: OpenAI API 响应体字典，预期包含 "data" 数组.

    Returns:
        有效 model_id 字符串列表. 若 "data" 字段缺失则返回空列表.
    """
    return _extract_ids_from_data(data)


def parse_anthropic_model_list(data: dict) -> list[str]:
    """解析 Anthropic 格式的模型列表响应.

    从 JSON 响应体的 "data" 数组中提取每个对象的 "id" 字段作为 model_id。

    Args:
        data: Anthropic API 响应体字典，预期包含 "data" 数组.

    Returns:
        有效 model_id 字符串列表. 若 "data" 字段缺失则返回空列表.
    """
    return _extract_ids_from_data(data)


def parse_gemini_model_list(data: dict) -> list[str]:
    """解析 Gemini 格式的模型列表响应.

    从 JSON 响应体的 "models" 数组中提取每个对象的 "name" 字段。
    若 "name" 值以 "models/" 开头则去除该前缀，否则直接使用完整值。

    Args:
        data: Gemini API 响应体字典，预期包含 "models" 数组.

    Returns:
        有效 model_id 字符串列表. 若 "models" 字段缺失则返回空列表.
    """
    items = data.get("models")
    if not isinstance(items, list):
        return []

    result: list[str] = []
    for item in items:
        if not isinstance(item, dict):
            continue
        name = item.get("name")
        if not isinstance(name, str) or name == "":
            continue
        # 去除 "models/" 前缀
        if name.startswith("models/"):
            name = name[len("models/"):]
        # 去除前缀后若为空字符串则跳过
        if name == "":
            continue
        result.append(name)
    return result


def parse_azure_model_list(data: dict) -> list[str]:
    """解析 Azure 格式的模型列表响应.

    从 JSON 响应体的 "data" 数组中提取每个对象的 "id" 字段作为 model_id。

    Args:
        data: Azure API 响应体字典，预期包含 "data" 数组.

    Returns:
        有效 model_id 字符串列表. 若 "data" 字段缺失则返回空列表.
    """
    return _extract_ids_from_data(data)
