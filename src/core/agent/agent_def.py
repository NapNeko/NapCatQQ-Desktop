# -*- coding: utf-8 -*-
"""Agent 定义模型.

定义 AgentDefinition pydantic 模型, 描述一个 Agent 的名称, 描述, 模式, 
系统提示词, 关联工具列表和权限规则列表. 
"""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, Field

from src.core.agent.permission import PermissionRule


class AgentDefinition(BaseModel):
    """Agent 定义.

    Attributes:
        name: Agent 名称, 最大 64 字符.
        description: Agent 描述, 最大 256 字符.
        mode: Agent 模式, primary (主 Agent) 或 subagent (子 Agent) .
        system_prompt: 系统提示词, 最大 16384 字符.
        tool_ids: 关联的工具 ID 列表, 最多 50 个.
        permission_rules: 权限规则列表, 最多 30 条.
    """

    name: str = Field(max_length=64)
    description: str = Field(max_length=256)
    mode: Literal["primary", "subagent"]
    system_prompt: str = Field(max_length=16384)
    tool_ids: list[str] = Field(default_factory=list, max_length=50)
    permission_rules: list[PermissionRule] = Field(default_factory=list, max_length=30)
