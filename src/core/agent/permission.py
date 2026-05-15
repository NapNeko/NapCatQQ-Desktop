# -*- coding: utf-8 -*-
"""权限规则与匹配引擎.

定义 PermissionRule 数据模型和权限匹配引擎, 支持 glob pattern 匹配, 
按 specificity (非通配符字面字符数) 降序排列规则, 第一个匹配的规则生效. 
无匹配规则时默认拒绝. 
"""

from __future__ import annotations

import re
from typing import Literal

from pydantic import BaseModel, Field


class PermissionRule(BaseModel):
    """权限规则.

    Attributes:
        pattern: glob 模式字符串, 支持 * 和 ? 通配符, 最大 256 字符.
        target: 工具 ID 或资源路径.
        action: 权限动作, allow (允许) , deny (拒绝) 或 ask (询问用户) .
    """

    pattern: str = Field(max_length=256)
    target: str
    action: Literal["allow", "deny", "ask"]


def _compute_specificity(pattern: str) -> int:
    """计算 pattern 的 specificity (非通配符字面字符数) .

    统计 pattern 中不是 '*' 和 '?' 的字符数量. 
    specificity 越高表示规则越具体. 

    Args:
        pattern: glob 模式字符串.

    Returns:
        非通配符字面字符的数量.
    """
    return sum(1 for ch in pattern if ch not in ("*", "?"))


def _glob_to_regex(pattern: str) -> re.Pattern[str]:
    """将 glob pattern 转换为正则表达式.

    支持的通配符: 
    - *: 匹配任意数量的任意字符 (包括零个) 
    - ?: 匹配恰好一个任意字符

    其他字符按字面值匹配 (会被 re.escape 转义) . 

    Args:
        pattern: glob 模式字符串.

    Returns:
        编译后的正则表达式对象.
    """
    regex_parts: list[str] = []
    i = 0
    while i < len(pattern):
        ch = pattern[i]
        if ch == "*":
            regex_parts.append(".*")
        elif ch == "?":
            regex_parts.append(".")
        else:
            regex_parts.append(re.escape(ch))
        i += 1
    return re.compile(f"^{''.join(regex_parts)}$")


def _matches_glob(pattern: str, value: str) -> bool:
    """检查 value 是否匹配 glob pattern.

    Args:
        pattern: glob 模式字符串.
        value: 要匹配的字符串.

    Returns:
        是否匹配.
    """
    regex = _glob_to_regex(pattern)
    return regex.match(value) is not None


def evaluate_permission(
    tool_id: str, rules: list[PermissionRule]
) -> Literal["allow", "deny", "ask"]:
    """评估工具调用的权限.

    按 specificity (非通配符字面字符数) 降序排列规则, 
    第一个 pattern 匹配 tool_id 的规则生效. 
    如果没有任何规则匹配, 默认拒绝 (deny) . 

    Args:
        tool_id: 要评估权限的工具 ID.
        rules: 权限规则列表.

    Returns:
        权限动作: "allow", "deny" 或 "ask".
    """
    # 按 specificity 降序排列 (更具体的规则优先) 
    sorted_rules = sorted(rules, key=lambda r: _compute_specificity(r.pattern), reverse=True)

    for rule in sorted_rules:
        if _matches_glob(rule.pattern, tool_id):
            return rule.action

    # 无匹配规则时默认拒绝
    return "deny"
