# -*- coding: utf-8 -*-
"""模型 ID 分组解析.

从 model_id 中解析 group_name, 用于在模型管理弹窗中按模型族分组展示.
"""

from __future__ import annotations


def parse_group_name(model_id: str) -> str:
    """从 model_id 解析 group_name.

    规则:
        1. 以连字符 ("-") 分割 model_id 为有序段列表
        2. 从第一段开始, 连续保留所有非纯数字且不以数字开头的段
        3. 遇到第一个纯数字段或以数字开头的段时停止
        4. 将保留的连续前缀段用连字符重新连接作为 group_name
        5. 若 model_id 不包含连字符, 返回完整 model_id
        6. 若所有段均为纯数字或以数字开头 (无有效前缀段), 返回完整 model_id

    不变式: model_id.startswith(group_name) 或 group_name == model_id

    Args:
        model_id: 模型标识符字符串.

    Returns:
        解析得到的 group_name.
    """
    if "-" not in model_id:
        return model_id

    segments = model_id.split("-")
    prefix_segments: list[str] = []

    for segment in segments:
        # 跳过空段 (连续连字符产生的空字符串)
        # 空段不是纯数字也不以数字开头, 但为了语义正确性, 空段视为无效前缀段
        if not segment:
            # 空段: 不是有效的非数字前缀, 停止
            break
        # 检查是否为纯数字
        if segment.isdigit():
            break
        # 检查是否以数字开头
        if segment[0].isdigit():
            break
        prefix_segments.append(segment)

    if not prefix_segments:
        return model_id

    return "-".join(prefix_segments)
