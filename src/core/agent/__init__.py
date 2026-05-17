# -*- coding: utf-8 -*-
"""Agent 能力框架模块.

提供 LLM Provider 管理, Tool 注册与执行, Session 会话管理, Agent 定义与调度等核心能力, 
为 NapCat 插件开发者提供 AI 辅助功能. 

本模块通过 creart 依赖注入框架注册所有核心组件的 Creator, 实现模块间解耦. 
"""

from abc import ABC

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module


class ProviderRegistryCreator(AbstractCreator, ABC):
    """ProviderRegistry 创建器"""

    targets = (
        CreateTargetInfo(
            module="src.core.agent.provider",
            identify="ProviderRegistry",
            humanized_name="Provider 注册表",
            description="LLM 提供商注册与管理",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断 Provider 模块是否可用"""
        return exists_module("src.core.agent.provider")

    @staticmethod
    def create(create_type):
        """创建 ProviderRegistry 实例并从磁盘恢复已保存的配置."""
        from creart import it

        from src.core.agent.config_persistence import ConfigPersistence
        from src.core.agent.default_providers import get_default_providers
        from src.core.runtime.paths import PathFunc

        registry = create_type()

        # 从磁盘加载已保存的 Provider 配置
        try:
            path_func: PathFunc = it(PathFunc)
            config_file_path = path_func.config_dir_path / "agent_config.json"
            persistence = ConfigPersistence(config_file_path)
            config_data = persistence.load()

            if config_data.providers:
                for provider in config_data.providers:
                    try:
                        registry.register(provider)
                    except Exception:
                        pass

                # 恢复活跃 Provider 和模型选择
                if config_data.active_provider_id and config_data.active_model_id:
                    try:
                        registry.set_active(
                            config_data.active_provider_id,
                            config_data.active_model_id,
                        )
                    except Exception:
                        pass
            else:
                # 首次启动: 无已保存配置, 注入默认供应商
                for provider in get_default_providers():
                    try:
                        registry.register(provider)
                    except Exception:
                        pass
        except Exception:
            # 加载失败时注入默认供应商, 确保 registry 不为空
            for provider in get_default_providers():
                try:
                    registry.register(provider)
                except Exception:
                    pass

        return registry


add_creator(ProviderRegistryCreator)


class ToolRegistryCreator(AbstractCreator, ABC):
    """ToolRegistry 创建器"""

    targets = (
        CreateTargetInfo(
            module="src.core.agent.tool",
            identify="ToolRegistry",
            humanized_name="Tool 注册表",
            description="工具注册与执行管理",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断 Tool 模块是否可用"""
        return exists_module("src.core.agent.tool")

    @staticmethod
    def create(create_type):
        """创建 ToolRegistry 实例"""
        return create_type()


add_creator(ToolRegistryCreator)


class SessionManagerCreator(AbstractCreator, ABC):
    """SessionManager 创建器"""

    targets = (
        CreateTargetInfo(
            module="src.core.agent.session",
            identify="SessionManager",
            humanized_name="Session 管理器",
            description="会话创建、持久化与管理",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断 Session 模块是否可用"""
        return exists_module("src.core.agent.session")

    @staticmethod
    def create(create_type):
        """创建 SessionManager 实例 (使用 PathFunc 提供的存储路径) """
        from creart import it

        from src.core.runtime.paths import PathFunc

        path_func = it(PathFunc)
        storage_dir = path_func.config_dir_path / "agent_sessions"
        return create_type(storage_dir=storage_dir)


add_creator(SessionManagerCreator)


class AgentEngineCreator(AbstractCreator, ABC):
    """AgentEngine 创建器"""

    targets = (
        CreateTargetInfo(
            module="src.core.agent.engine",
            identify="AgentEngine",
            humanized_name="Agent 引擎",
            description="Agent 核心调度引擎",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断 AgentEngine 模块是否可用"""
        return exists_module("src.core.agent.engine")

    @staticmethod
    def create(create_type):
        """创建 AgentEngine 实例 (通过 creart.it() 获取依赖注入) """
        from creart import it

        from src.core.agent.provider import ProviderRegistry
        from src.core.agent.session import SessionManager
        from src.core.agent.tool import ToolRegistry

        provider_registry = it(ProviderRegistry)
        tool_registry = it(ToolRegistry)
        session_manager = it(SessionManager)
        return create_type(provider_registry, tool_registry, session_manager)


add_creator(AgentEngineCreator)
