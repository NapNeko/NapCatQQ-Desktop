# -*- coding: utf-8 -*-
"""NapCat WebUI API 工具集.

实现 webui_list_plugins, webui_reload_plugin, webui_plugin_config, 
webui_bot_info, webui_send_test_message 五个内置工具, 
通过 WebUIClientInterface 协议与 NapCat WebUI HTTP API 交互. 
"""

from __future__ import annotations

from typing import Any, Protocol, runtime_checkable

from pydantic import BaseModel, Field

from src.core.agent.tool import ToolDefinition, ToolResult


# ---------------------------------------------------------------------------
# WebUI Client Interface (Protocol)
# ---------------------------------------------------------------------------


@runtime_checkable
class WebUIClientInterface(Protocol):
    """NapCat WebUI 客户端协议.

    定义 Agent 工具所需的 WebUI API 操作接口. 
    实现者应复用 NapCatWebUIClient 基础设施. 
    """

    async def list_plugins(self) -> list[dict[str, Any]]:
        """列出所有已安装插件."""
        ...

    async def reload_plugin(self, plugin_id: str) -> dict[str, Any]:
        """热重载指定插件."""
        ...

    async def get_plugin_config(self, plugin_id: str) -> dict[str, Any]:
        """读取插件配置."""
        ...

    async def set_plugin_config(self, plugin_id: str, config: dict[str, Any]) -> dict[str, Any]:
        """写入插件配置."""
        ...

    async def get_bot_info(self) -> dict[str, Any]:
        """查询 Bot 实例信息."""
        ...

    async def send_message(self, target_type: str, target_id: int, message: str) -> dict[str, Any]:
        """发送测试消息."""
        ...


# ---------------------------------------------------------------------------
# Error handling helpers
# ---------------------------------------------------------------------------

# 连接失败提示信息
_CONNECTION_ERROR_MSG = (
    "无法连接到 NapCat WebUI，请检查：\n"
    "1. NapCat 是否正在运行\n"
    "2. WebUI 功能是否已启用\n"
    "3. WebUI 端口配置是否正确"
)

# 认证失败提示信息
_AUTH_ERROR_MSG = (
    "NapCat WebUI 认证失败，请检查：\n"
    "1. WebUI token 配置是否正确\n"
    "2. token 是否已过期\n"
    "3. NapCat 设置中的 WebUI 认证配置"
)


class AuthenticationError(Exception):
    """WebUI 认证失败异常.

    用于标识 401/403 等认证相关错误. 
    """

    pass


def _handle_webui_error(exc: Exception) -> ToolResult:
    """统一处理 WebUI 调用异常, 返回适当的 ToolResult.

    Args:
        exc: 捕获到的异常.

    Returns:
        包含错误信息的 ToolResult(is_error=True).
    """
    if isinstance(exc, (ConnectionError, TimeoutError, OSError)):
        return ToolResult(output=_CONNECTION_ERROR_MSG, is_error=True)
    if isinstance(exc, AuthenticationError):
        return ToolResult(output=_AUTH_ERROR_MSG, is_error=True)
    # 检查异常消息中是否包含认证相关关键词
    exc_msg = str(exc).lower()
    if "unauthorized" in exc_msg or "401" in exc_msg or "403" in exc_msg:
        return ToolResult(output=_AUTH_ERROR_MSG, is_error=True)
    # 检查连接相关关键词
    if any(
        kw in exc_msg
        for kw in ("connection refused", "connect", "timeout", "unreachable")
    ):
        return ToolResult(output=_CONNECTION_ERROR_MSG, is_error=True)
    # 其他未知错误
    return ToolResult(
        output=f"WebUI API 调用失败: {type(exc).__name__}: {exc}",
        is_error=True,
    )


# ---------------------------------------------------------------------------
# Parameter models
# ---------------------------------------------------------------------------


class WebuiListPluginsParams(BaseModel):
    """webui_list_plugins 工具参数 (无参数) ."""

    pass


class WebuiReloadPluginParams(BaseModel):
    """webui_reload_plugin 工具参数."""

    plugin_id: str = Field(description="要热重载的插件 ID")


class WebuiPluginConfigParams(BaseModel):
    """webui_plugin_config 工具参数."""

    plugin_id: str = Field(description="插件 ID")
    config_data: dict[str, Any] | None = Field(
        default=None,
        description="要写入的配置数据（为 None 时表示读取配置）",
    )


class WebuiBotInfoParams(BaseModel):
    """webui_bot_info 工具参数 (无参数) ."""

    pass


class WebuiSendTestMessageParams(BaseModel):
    """webui_send_test_message 工具参数."""

    target_type: str = Field(description="目标类型: 'group' 或 'user'")
    target_id: int = Field(description="目标 ID（群号或 QQ 号）")
    message: str = Field(description="要发送的测试消息内容")


# ---------------------------------------------------------------------------
# WebuiListPluginsTool
# ---------------------------------------------------------------------------


class WebuiListPluginsTool(ToolDefinition):
    """列出 NapCat 已安装的所有插件.

    返回每个插件的 id, name, version, loaded 和 enabled 状态. 
    """

    tool_id = "webui_list_plugins"
    description = "列出 NapCat 已安装的所有插件，返回插件 ID、名称、版本、加载状态和启用状态"
    parameters_schema = WebuiListPluginsParams

    def __init__(self, webui_client: WebUIClientInterface) -> None:
        self._client = webui_client

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行插件列表查询."""
        try:
            plugins = await self._client.list_plugins()
        except Exception as exc:
            return _handle_webui_error(exc)

        if not plugins:
            return ToolResult(output="当前没有已安装的插件")

        # 格式化输出
        lines: list[str] = [f"已安装插件 ({len(plugins)} 个):"]
        lines.append("")
        for plugin in plugins:
            pid = plugin.get("id", "unknown")
            name = plugin.get("name", pid)
            version = plugin.get("version", "unknown")
            loaded = "已加载" if plugin.get("loaded", False) else "未加载"
            enabled = "已启用" if plugin.get("enabled", False) else "已禁用"
            lines.append(f"  - {name} (id={pid}, v{version}) [{loaded}, {enabled}]")

        return ToolResult(
            output="\n".join(lines),
            metadata={"plugin_count": len(plugins)},
        )


# ---------------------------------------------------------------------------
# WebuiReloadPluginTool
# ---------------------------------------------------------------------------


class WebuiReloadPluginTool(ToolDefinition):
    """热重载指定的 NapCat 插件.

    触发运行中的 NapCat 实例对指定插件执行热重载. 
    """

    tool_id = "webui_reload_plugin"
    description = "热重载指定的 NapCat 插件，触发运行中的 NapCat 实例重新加载插件代码"
    parameters_schema = WebuiReloadPluginParams

    def __init__(self, webui_client: WebUIClientInterface) -> None:
        self._client = webui_client

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行插件热重载."""
        assert isinstance(params, WebuiReloadPluginParams)

        try:
            result = await self._client.reload_plugin(params.plugin_id)
        except Exception as exc:
            return _handle_webui_error(exc)

        message = result.get("message", "操作完成")
        return ToolResult(
            output=f"插件 '{params.plugin_id}' 热重载成功: {message}",
            metadata={"plugin_id": params.plugin_id},
        )


# ---------------------------------------------------------------------------
# WebuiPluginConfigTool
# ---------------------------------------------------------------------------


class WebuiPluginConfigTool(ToolDefinition):
    """读取或写入 NapCat 插件配置.

    当 config_data 为 None 时读取配置, 否则写入配置. 
    """

    tool_id = "webui_plugin_config"
    description = "读取或写入 NapCat 插件配置（config_data 为空时读取，非空时写入）"
    parameters_schema = WebuiPluginConfigParams

    def __init__(self, webui_client: WebUIClientInterface) -> None:
        self._client = webui_client

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行插件配置读写."""
        assert isinstance(params, WebuiPluginConfigParams)

        try:
            if params.config_data is None:
                # 读取配置
                config = await self._client.get_plugin_config(params.plugin_id)
                import json

                config_str = json.dumps(config, ensure_ascii=False, indent=2)
                return ToolResult(
                    output=f"插件 '{params.plugin_id}' 当前配置:\n{config_str}",
                    metadata={"plugin_id": params.plugin_id, "action": "read"},
                )
            else:
                # 写入配置
                result = await self._client.set_plugin_config(
                    params.plugin_id, params.config_data
                )
                message = result.get("message", "操作完成")
                return ToolResult(
                    output=f"插件 '{params.plugin_id}' 配置已更新: {message}",
                    metadata={"plugin_id": params.plugin_id, "action": "write"},
                )
        except Exception as exc:
            return _handle_webui_error(exc)


# ---------------------------------------------------------------------------
# WebuiBotInfoTool
# ---------------------------------------------------------------------------


class WebuiBotInfoTool(ToolDefinition):
    """查询 NapCat Bot 实例信息.

    返回 Bot 的在线状态, QQ 号, 昵称和已连接群数量. 
    """

    tool_id = "webui_bot_info"
    description = "查询 NapCat Bot 实例信息，包括在线状态、QQ 号、昵称和已连接群数量"
    parameters_schema = WebuiBotInfoParams

    def __init__(self, webui_client: WebUIClientInterface) -> None:
        self._client = webui_client

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行 Bot 信息查询."""
        try:
            info = await self._client.get_bot_info()
        except Exception as exc:
            return _handle_webui_error(exc)

        # 格式化输出
        online = "在线" if info.get("online", False) else "离线"
        qq_number = info.get("qq", "未知")
        nickname = info.get("nickname", "未知")
        groups_count = info.get("groups_count", 0)

        lines = [
            "Bot 实例信息:",
            f"  状态: {online}",
            f"  QQ 号: {qq_number}",
            f"  昵称: {nickname}",
            f"  已连接群数: {groups_count}",
        ]

        return ToolResult(
            output="\n".join(lines),
            metadata={
                "online": info.get("online", False),
                "qq": qq_number,
                "nickname": nickname,
                "groups_count": groups_count,
            },
        )


# ---------------------------------------------------------------------------
# WebuiSendTestMessageTool
# ---------------------------------------------------------------------------


class WebuiSendTestMessageTool(ToolDefinition):
    """发送测试消息到指定目标.

    通过 NapCat WebUI API 发送测试消息到群或用户, 用于插件开发测试. 
    """

    tool_id = "webui_send_test_message"
    description = "通过 NapCat 发送测试消息到指定群或用户，用于插件开发测试"
    parameters_schema = WebuiSendTestMessageParams

    def __init__(self, webui_client: WebUIClientInterface) -> None:
        self._client = webui_client

    async def execute(self, params: BaseModel) -> ToolResult:
        """执行测试消息发送."""
        assert isinstance(params, WebuiSendTestMessageParams)

        # 验证 target_type
        if params.target_type not in ("group", "user"):
            return ToolResult(
                output=f"无效的目标类型 '{params.target_type}'，必须为 'group' 或 'user'",
                is_error=True,
            )

        try:
            result = await self._client.send_message(
                params.target_type, params.target_id, params.message
            )
        except Exception as exc:
            return _handle_webui_error(exc)

        target_desc = "群" if params.target_type == "group" else "用户"
        message_id = result.get("message_id", "")
        msg_info = f" (message_id={message_id})" if message_id else ""

        return ToolResult(
            output=f"测试消息已发送到{target_desc} {params.target_id}{msg_info}",
            metadata={
                "target_type": params.target_type,
                "target_id": params.target_id,
                "message_id": message_id,
            },
        )
