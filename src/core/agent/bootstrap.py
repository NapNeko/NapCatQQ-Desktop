# -*- coding: utf-8 -*-
"""Agent 引擎启动引导模块.

提供 bootstrap_agent_engine() 函数，作为 Agent 能力框架的集成入口点，
负责创建所有核心组件、注册内置工具、加载持久化配置并返回就绪的 AgentEngine 实例。

Requirements: 6.2, 6.3, 6.4, 6.5, 10.2, 13.6
"""

from __future__ import annotations

import logging
from pathlib import Path
from typing import Protocol, runtime_checkable

from src.core.agent.config_persistence import ConfigPersistence
from src.core.agent.context_loader import ContextLoader
from src.core.agent.default_providers import get_default_providers
from src.core.agent.engine import AgentEngine
from src.core.agent.provider import ProviderRegistry
from src.core.agent.session import SessionManager
from src.core.agent.tool import ToolProvider, ToolRegistry
from src.core.agent.tools.bot_tools import (
    BotLogsTool,
    BotManagerInterface,
    BotRestartTool,
    BotStartTool,
    BotStatusTool,
    BotStopTool,
)
from src.core.agent.tools.context_tools import OpenContextFileTool
from src.core.agent.tools.file_tools import FileEditTool, FileReadTool, FileWriteTool
from src.core.agent.tools.search_tools import GrepSearchTool, ListDirectoryTool
from src.core.agent.tools.shell_tools import ShellExecTool
from src.core.agent.tools.webui_tools import (
    WebUIClientInterface,
    WebuiBotInfoTool,
    WebuiListPluginsTool,
    WebuiPluginConfigTool,
    WebuiReloadPluginTool,
    WebuiSendTestMessageTool,
)

logger = logging.getLogger(__name__)


def register_builtin_tools(
    tool_registry: ToolRegistry,
    workspace_dir: Path,
    config_dir: Path,
    bot_manager: BotManagerInterface | None = None,
    webui_client: WebUIClientInterface | None = None,
) -> None:
    """注册所有内置工具到 ToolRegistry.

    将文件操作、搜索、Shell 执行、Bot 管理、WebUI API 和上下文工具
    全部实例化并注册到工具注册表中。

    Args:
        tool_registry: 工具注册表实例.
        workspace_dir: 工作区根目录（用于文件/搜索/Shell 工具）.
        config_dir: 配置目录（用于上下文文件工具）.
        bot_manager: Bot 进程管理器接口（可选，为 None 时跳过 Bot 工具注册）.
        webui_client: WebUI 客户端接口（可选，为 None 时跳过 WebUI 工具注册）.
    """
    resolved_workspace = workspace_dir.resolve()
    context_file_path = config_dir / "agent_context.md"

    # 1. 文件操作工具
    tool_registry.register(FileReadTool(resolved_workspace))
    tool_registry.register(FileWriteTool(resolved_workspace))
    tool_registry.register(FileEditTool(resolved_workspace))

    # 2. 搜索工具
    tool_registry.register(GrepSearchTool(resolved_workspace))
    tool_registry.register(ListDirectoryTool(resolved_workspace))

    # 3. Shell 执行工具
    tool_registry.register(ShellExecTool(resolved_workspace))

    # 4. Bot 管理工具（需要 BotManagerInterface 实例）
    if bot_manager is not None:
        tool_registry.register(BotStatusTool(bot_manager))
        tool_registry.register(BotStartTool(bot_manager))
        tool_registry.register(BotStopTool(bot_manager))
        tool_registry.register(BotRestartTool(bot_manager))
        tool_registry.register(BotLogsTool(bot_manager))
    else:
        logger.info("BotManagerInterface 未提供，跳过 Bot 管理工具注册。")

    # 5. WebUI API 工具（需要 WebUIClientInterface 实例）
    if webui_client is not None:
        tool_registry.register(WebuiListPluginsTool(webui_client))
        tool_registry.register(WebuiReloadPluginTool(webui_client))
        tool_registry.register(WebuiPluginConfigTool(webui_client))
        tool_registry.register(WebuiBotInfoTool(webui_client))
        tool_registry.register(WebuiSendTestMessageTool(webui_client))
    else:
        logger.info("WebUIClientInterface 未提供，跳过 WebUI 工具注册。")

    # 6. 用户上下文工具
    tool_registry.register(OpenContextFileTool(context_file_path))

    logger.info(
        "内置工具注册完成，共 %d 个工具已注册。",
        len(tool_registry.list_all()),
    )


def restore_config(
    config_persistence: ConfigPersistence,
    provider_registry: ProviderRegistry,
    engine: AgentEngine,
) -> None:
    """从持久化配置文件恢复 Provider 和 Agent 状态.

    加载 agent_config.json 中保存的 Provider 配置、Agent 定义和活跃模型选择，
    将它们恢复到对应的注册表中。

    容错策略：
    - 配置文件不存在或损坏时使用默认配置（由 ConfigPersistence 处理）
    - 恢复单个 Provider 失败时跳过并记录警告，不影响其他 Provider
    - 恢复活跃模型失败时记录警告，保持无活跃状态

    Args:
        config_persistence: 配置持久化管理器实例.
        provider_registry: Provider 注册表实例.
        engine: AgentEngine 实例（用于注册恢复的 Agent）.
    """
    config_data = config_persistence.load()

    # 恢复 Provider 配置
    for provider in config_data.providers:
        try:
            provider_registry.register(provider)
        except Exception as exc:
            logger.warning(
                "恢复 Provider '%s' 失败: %s",
                provider.provider_id,
                exc,
            )

    # 首次启动时注册默认供应商（配置文件中无任何 provider）
    if not config_data.providers:
        logger.info("未检测到已有供应商配置，注册默认供应商列表。")
        for provider in get_default_providers():
            try:
                provider_registry.register(provider)
            except Exception as exc:
                logger.warning(
                    "注册默认 Provider '%s' 失败: %s",
                    provider.provider_id,
                    exc,
                )

    # 恢复 Agent 定义（跳过默认 Agent，因为 engine 已自动注册）
    for agent_def in config_data.agents:
        if agent_def.name == "napcat-plugin-dev":
            # 默认 Agent 已在 engine.__init__ 中注册，跳过
            continue
        try:
            engine.register_agent(agent_def)
        except Exception as exc:
            logger.warning(
                "恢复 Agent '%s' 失败: %s",
                agent_def.name,
                exc,
            )

    # 恢复活跃 Provider 和模型选择
    if config_data.active_provider_id and config_data.active_model_id:
        try:
            provider_registry.set_active(
                config_data.active_provider_id,
                config_data.active_model_id,
            )
            logger.info(
                "已恢复活跃模型: provider=%s, model=%s",
                config_data.active_provider_id,
                config_data.active_model_id,
            )
        except Exception as exc:
            logger.warning(
                "恢复活跃模型失败 (provider=%s, model=%s): %s",
                config_data.active_provider_id,
                config_data.active_model_id,
                exc,
            )


def bootstrap_agent_engine(
    workspace_dir: Path,
    config_dir: Path,
    bot_manager: BotManagerInterface | None = None,
    webui_client: WebUIClientInterface | None = None,
    tool_providers: list[ToolProvider] | None = None,
) -> AgentEngine:
    """引导创建并初始化完整的 AgentEngine 实例.

    这是 Agent 能力框架的集成入口点，负责：
    1. 创建 ProviderRegistry、ToolRegistry、SessionManager 核心组件
    2. 注册所有内置工具
    3. 注册外部 ToolProvider（MCP 扩展点）
    4. 加载持久化配置恢复 Provider/Agent 状态
    5. 创建并返回就绪的 AgentEngine 实例

    Args:
        workspace_dir: 工作区根目录（用于文件/搜索/Shell 工具的根路径）.
        config_dir: 配置目录（用于 session 存储、配置持久化和上下文文件）.
        bot_manager: Bot 进程管理器接口（可选）.
        webui_client: WebUI 客户端接口（可选）.
        tool_providers: 外部工具提供者列表（MCP 扩展点，可选）.

    Returns:
        完全初始化的 AgentEngine 实例，包含所有内置工具和恢复的配置状态.
    """
    # 确保配置目录存在
    config_dir.mkdir(parents=True, exist_ok=True)

    # 1. 创建核心组件
    provider_registry = ProviderRegistry()
    tool_registry = ToolRegistry()
    session_storage_dir = config_dir / "agent_sessions"
    session_manager = SessionManager(storage_dir=session_storage_dir)

    # 2. 注册所有内置工具
    register_builtin_tools(
        tool_registry=tool_registry,
        workspace_dir=workspace_dir,
        config_dir=config_dir,
        bot_manager=bot_manager,
        webui_client=webui_client,
    )

    # 3. 注册外部 ToolProvider（MCP 扩展点）
    if tool_providers:
        for provider in tool_providers:
            try:
                tool_registry.register_provider(provider)
                logger.info(
                    "外部 ToolProvider 注册成功，提供 %d 个工具。",
                    len(provider.get_tools()),
                )
            except Exception as exc:
                logger.warning("外部 ToolProvider 注册失败: %s", exc)

    # 4. 创建 ContextLoader
    context_file_path = config_dir / "agent_context.md"
    context_loader = ContextLoader(context_file_path)

    # 5. 创建 AgentEngine（内部会注册默认 napcat-plugin-dev Agent）
    engine = AgentEngine(
        provider_registry=provider_registry,
        tool_registry=tool_registry,
        session_manager=session_manager,
        context_loader=context_loader,
    )

    # 6. 加载持久化配置恢复状态
    config_file_path = config_dir / "agent_config.json"
    config_persistence = ConfigPersistence(config_file_path)
    restore_config(config_persistence, provider_registry, engine)

    logger.info(
        "AgentEngine 引导完成: %d 个工具, %d 个 Provider, %d 个 Agent",
        len(tool_registry.list_all()),
        len(provider_registry.list_all()),
        len(engine.list_agents()),
    )

    return engine
