# -*- coding: utf-8 -*-
from __future__ import annotations

"""独立接口调试页面。"""

import json
from abc import ABC
from typing import TYPE_CHECKING, Self

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtCore import QSize, Qt, QThreadPool
from PySide6.QtGui import QShortcut
from PySide6.QtWidgets import QHBoxLayout, QSizePolicy, QSplitter, QVBoxLayout, QWidget

from src.core.api_debug import (
    ActionDebugService,
    ApiDebugActionSchema,
    ApiDebugActionSession,
    ApiDebugAuthConfig,
    ApiDebugBotContext,
    ApiDebugContextService,
    ApiDebugExecutionResult,
    ApiDebugMode,
    ApiDebugSearchItem,
    ApiDebugWorkspaceStore,
    SchemaDefaultGenerator,
)
from src.core.config.operate_config import read_config
from src.ui.common.style_sheet import PageStyleSheet
from src.ui.components.info_bar import success_bar, warning_bar
from .shared import ApiDebugSearchDialog, CallableTask, pretty_json
from .widget import ActionCatalogPanel, ActionDetailPanel, ApiDebugTopBar

if TYPE_CHECKING:
    from src.ui.window.main_window import MainWindow


class ApiDebugPage(QWidget):
    """独立接口调试页面。"""

    STABLE_MINIMUM_SIZE_HINT = QSize(1120, 720)
    STABLE_SIZE_HINT = QSize(1120, 720)

    def __init__(self) -> None:
        super().__init__()
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Ignored)
        self.context_service = ApiDebugContextService(config_reader=read_config)
        self.workspace_store = ApiDebugWorkspaceStore()
        self.workspace_state = self.workspace_store.load()
        self.action_service = ActionDebugService()
        self.schema_generator = SchemaDefaultGenerator()
        self.contexts: list[ApiDebugBotContext] = []
        self.schemas: list[ApiDebugActionSchema] = []
        self.current_context: ApiDebugBotContext | None = None
        self.current_auth = ApiDebugAuthConfig()
        self.current_session: ApiDebugActionSession | None = None
        self._last_layout_width = -1
        self._last_splitter_mode = ""

    def initialize(self, parent: "MainWindow") -> Self:
        self.setParent(parent)
        self.setObjectName("api_debug_page")

        self._build_widgets()
        self._build_layout()
        self._expose_detail_aliases()
        self._bind_signals()

        self.catalog_panel.search_edit.setText(self.workspace_state.action_draft.search_query)
        self.reload_contexts()
        self._sync_responsive_layout(force=True)
        PageStyleSheet.API_DEBUG.apply(self)
        return self

    def _build_widgets(self) -> None:
        self.content_widget = QWidget(self)
        self.content_widget.setObjectName("api_debug_content")
        self.content_widget.setMaximumWidth(1680)
        self.content_widget.setSizePolicy(QSizePolicy.Policy.MinimumExpanding, QSizePolicy.Policy.Ignored)

        self.top_bar = ApiDebugTopBar(self)
        self.catalog_panel = ActionCatalogPanel(self)
        self.catalog_panel.setObjectName("ApiDebugSideCard")
        self.catalog_panel.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Ignored)
        self.search_dialog = ApiDebugSearchDialog(self)

        self.root_splitter = QSplitter(Qt.Orientation.Horizontal, self.content_widget)
        self.root_splitter.setChildrenCollapsible(False)
        self.root_splitter.setHandleWidth(8)
        self.root_splitter.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Ignored)
        self.root_splitter.addWidget(self.catalog_panel)

        self.detail_widget = QWidget(self.root_splitter)
        self.detail_widget.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Ignored)
        self.detail_panel = ActionDetailPanel(self.detail_widget)
        self.root_splitter.addWidget(self.detail_widget)

        self.catalog_panel.setMinimumWidth(300)
        self.detail_widget.setMinimumWidth(640)
        self.root_splitter.setStretchFactor(0, 3)
        self.root_splitter.setStretchFactor(1, 7)

    def _build_layout(self) -> None:
        detail_layout = QVBoxLayout(self.detail_widget)
        detail_layout.setContentsMargins(0, 0, 0, 0)
        detail_layout.setSpacing(0)
        detail_layout.addWidget(self.detail_panel, 1)

        content_layout = QVBoxLayout(self.content_widget)
        content_layout.setContentsMargins(0, 0, 0, 0)
        content_layout.setSpacing(16)
        content_layout.addWidget(self.top_bar)
        content_layout.addWidget(self.root_splitter, 1)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(24, 20, 24, 10)
        layout.setSpacing(16)
        layout.addWidget(self.content_widget)

    def _expose_detail_aliases(self) -> None:
        self.detail_card = self.detail_panel
        self.detail_state_stack = self.detail_panel.detail_state_stack
        self.detail_content_page = self.detail_panel.detail_content_page
        self.detail_empty_page = self.detail_panel.detail_empty_page
        self.action_title = self.detail_panel.action_title
        self.action_summary = self.detail_panel.action_summary
        self.action_tags = self.detail_panel.action_tags
        self.generate_button = self.detail_panel.generate_button
        self.send_button = self.detail_panel.send_button
        self.detail_pivot = self.detail_panel.detail_pivot
        self.detail_stack = self.detail_panel.detail_stack
        self.params_page = self.detail_panel.params_page
        self.docs_page = self.detail_panel.docs_page
        self.result_page = self.detail_panel.result_page
        self.params_editor = self.detail_panel.params_editor
        self.docs_view = self.detail_panel.docs_view
        self.result_card = self.detail_panel.result_card
        self.empty_title = self.detail_panel.empty_title
        self.empty_hint = self.detail_panel.empty_hint
        self.empty_container = self.detail_panel.empty_container

    def _bind_signals(self) -> None:
        self.top_bar.search_requested.connect(self._open_search)
        self.top_bar.refresh_requested.connect(self.reload_contexts)
        self.top_bar.bot_changed.connect(self._handle_bot_changed)
        self.catalog_panel.action_selected.connect(self._apply_schema)
        self.catalog_panel.search_changed.connect(self._handle_search_changed)
        self.generate_button.clicked.connect(self._generate_payload_for_current_action)
        self.send_button.clicked.connect(self._send_current_action)
        self.params_editor.textChanged.connect(self._sync_state)

        self.search_shortcut = QShortcut("Ctrl+K", self)
        self.search_shortcut.activated.connect(self._open_search)

    def resizeEvent(self, event) -> None:
        super().resizeEvent(event)
        self._sync_responsive_layout()

    def minimumSizeHint(self) -> QSize:
        return QSize(self.STABLE_MINIMUM_SIZE_HINT)

    def sizeHint(self) -> QSize:
        return QSize(self.STABLE_SIZE_HINT)

    def hasHeightForWidth(self) -> bool:
        return False

    def heightForWidth(self, width: int) -> int:
        _ = width
        return -1

    def reload_contexts(self) -> None:
        self.contexts = self.context_service.list_bot_contexts()
        selected_bot_id = self.workspace_state.selected_bot_id
        if self.contexts and not any(context.bot_id == selected_bot_id for context in self.contexts):
            selected_bot_id = self.contexts[0].bot_id
        elif not self.contexts:
            selected_bot_id = ""

        self.workspace_state.selected_bot_id = selected_bot_id
        self.top_bar.populate_bots(self.contexts, selected_bot_id)
        self._apply_context(self._current_context())
        self._persist_workspace()

    def _apply_context(self, context: ApiDebugBotContext | None) -> None:
        self.current_context = context
        self.current_session = None
        self.schemas = []
        self.catalog_panel.set_schemas([], "")
        self.result_card.reset()

        if context is None:
            self.current_auth = ApiDebugAuthConfig()
            self._set_unavailable_state("当前没有可调试的 Bot", "请先启动 Bot 和 WebUI。")
            return

        base_url = context.preferred_action_base_url()
        if not base_url:
            self.current_auth = ApiDebugAuthConfig()
            self._set_unavailable_state("当前 Bot 暂不可调试", "没有发现运行中的 WebUI Debug 接口。")
            return

        self.current_auth = ApiDebugAuthConfig.webui_session(
            token=context.webui_token,
            session_credential=context.webui_credential,
        )
        self._set_loading_state()
        self._run_async(
            lambda: self.action_service.fetch_schemas(base_url, self.current_auth),
            on_success=self._handle_schema_success,
            on_error=self._handle_schema_error,
        )

    def _handle_schema_success(self, schemas: list[ApiDebugActionSchema]) -> None:
        self.schemas = schemas
        if not schemas:
            self.catalog_panel.set_schemas([], "")
            self._set_unavailable_state("当前没有可调试接口", "运行中的 WebUI 没有返回任何 Action schema。")
            return

        selected_action = self.workspace_state.action_draft.action
        draft_params_text = self.workspace_state.action_draft.params_text
        if not any(schema.action == selected_action for schema in schemas):
            selected_action = schemas[0].action

        self.catalog_panel.set_schemas(schemas, selected_action)
        self.catalog_panel.set_selected_action(selected_action)
        current_schema = next((schema for schema in schemas if schema.action == selected_action), schemas[0])
        self._apply_schema(current_schema)
        if selected_action == self.workspace_state.action_draft.action and draft_params_text.strip():
            self.params_editor.setPlainText(draft_params_text)
        self._set_enabled_state(True)

    def _handle_schema_error(self, message: str) -> None:
        self.schemas = []
        self.catalog_panel.set_schemas([], "")
        self._set_unavailable_state("接口列表加载失败", message)
        warning_bar(message, title="接口调试不可用", parent=self)

    def _apply_schema(self, schema: ApiDebugActionSchema | None) -> None:
        if schema is None:
            self.workspace_state.action_draft.action = ""
            self._set_empty_detail_state("没有匹配的接口", "调整搜索条件或标签过滤后再试。")
            self._persist_workspace()
            return

        restore_existing_params = self.workspace_state.action_draft.action == schema.action and bool(
            self.workspace_state.action_draft.params_text.strip()
        )
        self.workspace_state.action_draft.action = schema.action
        self.action_title.setText(schema.action)
        self.action_summary.setText(schema.summary.strip() or schema.description.strip() or "暂无接口说明")
        self.action_tags.setText(f"标签: {', '.join(schema.action_tags)}" if schema.action_tags else "标签: 未分类")
        self.docs_view.setPlainText(self._build_docs_text(schema))
        if restore_existing_params:
            self.params_editor.setPlainText(self.workspace_state.action_draft.params_text)
        else:
            self._fill_default_params(schema)
        self.result_card.reset()
        self._set_enabled_state(True)
        self.detail_panel.show_detail_page("params")
        self._persist_workspace()

    def _fill_default_params(self, schema: ApiDebugActionSchema) -> None:
        payload = self.schema_generator.build_default(schema.payload_schema, schema.payload_example)
        self.params_editor.set_json(pretty_json(payload))

    def _generate_payload_for_current_action(self) -> None:
        schema = self._current_schema()
        if schema is None:
            return
        self._fill_default_params(schema)
        self._sync_state()

    def _send_current_action(self) -> None:
        schema = self._current_schema()
        if schema is None:
            warning_bar("请先选择一个接口", parent=self)
            return
        if self.current_context is None or not self.current_context.preferred_action_base_url():
            warning_bar("当前 Bot 没有可用的 WebUI Debug 接口", parent=self)
            return
        if not self.params_editor.check_json(show_tips=False):
            warning_bar("参数 JSON 无效", title="无法发送", parent=self)
            return

        params = json.loads(self.params_editor.toPlainText() or "{}")
        self.send_button.setEnabled(False)
        self.send_button.setText("发送中...")
        self._run_async(
            lambda: self._execute_action(schema.action, params),
            on_success=self._handle_execute_result,
            on_error=self._handle_execute_error,
            on_finished=self._reset_send_button,
        )

    def _execute_action(self, action: str, params: object) -> ApiDebugExecutionResult:
        if self.current_context is None:
            raise RuntimeError("当前没有可用的 Bot 上下文")

        base_url = self.current_context.preferred_action_base_url()
        if not base_url:
            raise RuntimeError("当前 Bot 没有运行中的 WebUI Debug 接口")

        if self.current_session is None or self.current_session.base_url != base_url:
            self.current_session = self.action_service.create_session(base_url, self.current_auth)

        return self.action_service.call_action(
            self.current_session,
            action,
            params,
            self.current_auth,
            persist_history=False,
        )

    def _handle_execute_result(self, result: ApiDebugExecutionResult) -> None:
        self.result_card.apply_result(result)
        self.detail_panel.show_detail_page("result")
        if result.error is None:
            success_bar("接口调用成功", parent=self)
        else:
            warning_bar(result.error.message, title="接口调用失败", parent=self)

    def _handle_execute_error(self, message: str) -> None:
        self.current_session = None
        self.result_card.reset(message)
        self.detail_panel.show_detail_page("result")
        warning_bar(message, title="接口调用失败", parent=self)

    def _reset_send_button(self) -> None:
        self.send_button.setEnabled(True)
        self.send_button.setText("发送调试请求")

    def _open_search(self) -> None:
        items = [
            ApiDebugSearchItem(
                item_id=f"action:{schema.action}",
                title=schema.action,
                subtitle=schema.summary.strip() or "接口调试",
                mode=ApiDebugMode.ACTION,
                payload={"action": schema.action},
            )
            for schema in self.schemas
        ]
        self.search_dialog.set_items(items)
        chosen = self.search_dialog.open_and_choose()
        if chosen is None:
            return
        action_name = str(chosen.payload.get("action", "")).strip()
        if action_name:
            self.catalog_panel.set_selected_action(action_name)

    def _handle_bot_changed(self, bot_id: str) -> None:
        self.workspace_state.selected_bot_id = bot_id
        self._apply_context(self._current_context())
        self._persist_workspace()

    def _handle_search_changed(self, text: str) -> None:
        self.workspace_state.action_draft.search_query = text
        self._persist_workspace()

    def _sync_state(self) -> None:
        self.workspace_state.action_draft.params_text = self.params_editor.toPlainText()
        self._persist_workspace()

    def _persist_workspace(self) -> None:
        self.workspace_store.save(self.workspace_state)

    def _current_context(self) -> ApiDebugBotContext | None:
        return next(
            (context for context in self.contexts if context.bot_id == self.workspace_state.selected_bot_id), None
        )

    def _current_schema(self) -> ApiDebugActionSchema | None:
        action_name = self.workspace_state.action_draft.action
        return next((schema for schema in self.schemas if schema.action == action_name), None)

    def _set_loading_state(self) -> None:
        self._set_enabled_state(False)
        self.detail_panel.set_empty_state("正在加载接口...", "正在从当前 Bot 的 WebUI Debug 接口获取 Action schema。")

    def _set_unavailable_state(self, title: str, message: str) -> None:
        self._set_enabled_state(False)
        self.detail_panel.set_empty_state(title, message)

    def _set_empty_detail_state(self, title: str, message: str) -> None:
        self._set_enabled_state(False)
        self.detail_panel.set_empty_state(title, message)

    def _set_enabled_state(self, enabled: bool) -> None:
        self.detail_panel.set_enabled_state(enabled)

    def _build_docs_text(self, schema: ApiDebugActionSchema) -> str:
        sections = [
            f"接口名称\n{schema.action}",
            f"摘要\n{schema.summary.strip() or '暂无'}",
            f"说明\n{schema.description.strip() or schema.summary.strip() or '暂无'}",
            f"标签\n{', '.join(schema.action_tags) if schema.action_tags else '未分类'}",
            f"请求 Schema\n{pretty_json(schema.payload_schema if schema.payload_schema is not None else {})}",
            f"返回 Schema\n{pretty_json(schema.return_schema if schema.return_schema is not None else {})}",
        ]
        return "\n\n".join(sections)

    def _sync_responsive_layout(self, *, force: bool = False) -> None:
        available_width = self._content_width()
        if not force and available_width == self._last_layout_width:
            return

        self._last_layout_width = available_width
        self.top_bar.sync_layout(available_width)
        splitter_mode = "wide" if available_width >= 1360 else "default"
        self.root_splitter.setOrientation(Qt.Orientation.Horizontal)

        if force or splitter_mode != self._last_splitter_mode:
            left_width = 320 if splitter_mode == "wide" else 300
            right_width = max(720, available_width - left_width - 24)
            self.root_splitter.setSizes([left_width, right_width])
            self._last_splitter_mode = splitter_mode

    def _content_width(self) -> int:
        outer_width = max(0, self.width() - 24)
        bounded_width = min(outer_width, self.content_widget.maximumWidth()) if hasattr(self, "content_widget") else 0
        return max(0, bounded_width - 24)

    def _run_async(
        self,
        func,
        *,
        on_success=None,
        on_error=None,
        on_finished=None,
    ) -> None:
        task = CallableTask(func)
        if on_success is not None:
            task.result_ready.connect(on_success)
        if on_error is not None:
            task.error_raised.connect(on_error)
        if on_finished is not None:
            task.finished.connect(on_finished)
        QThreadPool.globalInstance().start(task)


class ApiDebugPageCreator(AbstractCreator, ABC):
    """接口调试页面创建器。"""

    targets = (
        CreateTargetInfo(
            module="src.ui.page.api_debug_page",
            identify="ApiDebugPage",
            humanized_name="接口调试页面",
            description="NapCatQQ Desktop 接口调试页",
        ),
    )

    @staticmethod
    def available() -> bool:
        return exists_module("src.ui.page.api_debug_page")

    @staticmethod
    def create(create_type):
        return create_type()


add_creator(ApiDebugPageCreator)

__all__ = ["ApiDebugPage"]
