# -*- coding: utf-8 -*-
"""
Provider 配置面板 - 管理 LLM Provider 的增删改和模型选择
"""
from __future__ import annotations

from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    ComboBox,
    ExpandLayout,
    FluentIcon,
    HeaderCardWidget,
    InfoBadge,
    LineEdit,
    MessageBoxBase,
    PasswordLineEdit,
    PlainTextEdit,
    PrimaryPushButton,
    ScrollArea,
    SettingCardGroup,
    SubtitleLabel,
    TransparentToolButton,
)
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

if TYPE_CHECKING:
    from src.core.agent.provider import ProviderRegistry

from src.ui.components.info_bar import error_bar, success_bar


class AddProviderDialog(MessageBoxBase):
    """添加 Provider 的表单对话框

    提供 provider_id, name, api_base_url, api_key_ref, models 字段的输入,
    并在提交前进行表单验证.
    """

    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent=parent)
        self._setup_ui()
        self.widget.setMinimumWidth(480)

    def _setup_ui(self) -> None:
        """构建表单 UI"""
        self.title_label = SubtitleLabel(self.tr("添加 Provider"), self)
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addSpacing(8)

        # Provider ID
        self.provider_id_edit = LineEdit(self)
        self.provider_id_edit.setPlaceholderText(self.tr("唯一标识 (如: openai, deepseek)"))
        self.provider_id_edit.setClearButtonEnabled(True)
        self._add_form_row(self.tr("Provider ID"), self.provider_id_edit)

        # Name
        self.name_edit = LineEdit(self)
        self.name_edit.setPlaceholderText(self.tr("显示名称 (如: OpenAI, DeepSeek)"))
        self.name_edit.setClearButtonEnabled(True)
        self._add_form_row(self.tr("名称"), self.name_edit)

        # API Base URL
        self.api_base_url_edit = LineEdit(self)
        self.api_base_url_edit.setPlaceholderText(self.tr("https://api.openai.com/v1"))
        self.api_base_url_edit.setClearButtonEnabled(True)
        self._add_form_row(self.tr("API Base URL"), self.api_base_url_edit)

        # API Key Ref
        self.api_key_ref_edit = PasswordLineEdit(self)
        self.api_key_ref_edit.setPlaceholderText(self.tr("API 密钥或密钥引用"))
        self._add_form_row(self.tr("API Key"), self.api_key_ref_edit)

        # Models (one per line)
        self.viewLayout.addSpacing(4)
        models_hint = CaptionLabel(
            self.tr("模型列表 (每行一个, 格式: model_id,max_tokens[,display_name])"),
            self,
        )
        models_hint.setTextColor("#666666", "#999999")
        self.viewLayout.addWidget(models_hint)

        self.models_edit = PlainTextEdit(self)
        self.models_edit.setPlaceholderText(
            self.tr("gpt-4o,128000,GPT-4o\ngpt-4o-mini,16384,GPT-4o Mini")
        )
        self.models_edit.setFixedHeight(120)
        self.viewLayout.addWidget(self.models_edit)

        # 配置按钮文本
        self.yesButton.setText(self.tr("添加"))
        self.cancelButton.setText(self.tr("取消"))

    def _add_form_row(self, label_text: str, widget: QWidget) -> None:
        """添加一行表单: 标签 + 输入控件"""
        label = BodyLabel(label_text, self)
        self.viewLayout.addWidget(label)
        self.viewLayout.addWidget(widget)
        self.viewLayout.addSpacing(4)

    def validate(self) -> bool:
        """验证表单输入, 返回 False 阻止对话框关闭"""
        provider_id = self.provider_id_edit.text().strip()
        name = self.name_edit.text().strip()
        api_base_url = self.api_base_url_edit.text().strip()
        api_key_ref = self.api_key_ref_edit.text().strip()
        models_text = self.models_edit.toPlainText().strip()

        # 验证必填字段
        if not provider_id:
            error_bar(
                content=self.tr("Provider ID 不能为空"),
                title=self.tr("验证失败"),
                duration=3000,
                parent=self,
            )
            return False

        if not name:
            error_bar(
                content=self.tr("名称不能为空"),
                title=self.tr("验证失败"),
                duration=3000,
                parent=self,
            )
            return False

        if not api_base_url:
            error_bar(
                content=self.tr("API Base URL 不能为空"),
                title=self.tr("验证失败"),
                duration=3000,
                parent=self,
            )
            return False

        if not api_key_ref:
            error_bar(
                content=self.tr("API Key 不能为空"),
                title=self.tr("验证失败"),
                duration=3000,
                parent=self,
            )
            return False

        # 验证至少有一个模型
        if not models_text:
            error_bar(
                content=self.tr("至少需要添加一个模型"),
                title=self.tr("验证失败"),
                duration=3000,
                parent=self,
            )
            return False

        # 验证模型格式
        models = self._parse_models(models_text)
        if not models:
            error_bar(
                content=self.tr("模型格式错误, 每行格式: model_id,max_tokens[,display_name]"),
                title=self.tr("验证失败"),
                duration=3000,
                parent=self,
            )
            return False

        return True

    def _parse_models(self, models_text: str) -> list[dict] | None:
        """解析模型文本为模型列表

        每行格式: model_id,max_tokens[,display_name]

        Returns:
            解析成功返回模型字典列表, 失败返回 None
        """
        models: list[dict] = []
        for line in models_text.splitlines():
            line = line.strip()
            if not line:
                continue
            parts = [p.strip() for p in line.split(",")]
            if len(parts) < 2:
                return None
            model_id = parts[0]
            try:
                max_tokens = int(parts[1])
            except ValueError:
                return None
            if max_tokens < 1:
                return None
            display_name = parts[2] if len(parts) >= 3 else ""
            models.append({
                "model_id": model_id,
                "max_tokens": max_tokens,
                "display_name": display_name,
            })
        return models if models else None

    def get_provider_data(self) -> dict:
        """获取表单数据, 调用前应确保 validate() 通过

        Returns:
            包含 provider_id, name, api_base_url, api_key_ref, models 的字典
        """
        models_text = self.models_edit.toPlainText().strip()
        models = self._parse_models(models_text) or []
        return {
            "provider_id": self.provider_id_edit.text().strip(),
            "name": self.name_edit.text().strip(),
            "api_base_url": self.api_base_url_edit.text().strip(),
            "api_key_ref": self.api_key_ref_edit.text().strip(),
            "models": models,
        }


class ProviderConfigPanel(ScrollArea):
    """Provider 配置面板

    继承 qfluentwidgets.ScrollArea, 使用 SettingCardGroup 分组展示 Provider 列表,
    每个 Provider 使用 HeaderCardWidget 展示名称和模型列表.
    提供添加和删除 Provider 的功能.
    """

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent=parent)

        # 创建内部视图容器
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)

        # 配置 ScrollArea
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setObjectName("ProviderConfigPanel")
        self.view.setObjectName("ProviderConfigView")

        # 添加 Provider 按钮
        self.add_provider_button = PrimaryPushButton(
            FluentIcon.ADD, self.tr("添加 Provider"), self.view
        )
        self.add_provider_button.clicked.connect(self._on_add_provider_clicked)

        # 创建 Provider 列表分组
        self.provider_group = SettingCardGroup(
            title=self.tr("已注册 Provider"), parent=self.view
        )

        # 布局
        self.expand_layout.addWidget(self.add_provider_button)
        self.expand_layout.addWidget(self.provider_group)
        self.expand_layout.setContentsMargins(16, 16, 16, 16)
        self.view.setLayout(self.expand_layout)

        # 存储 provider 卡片映射: provider_id -> HeaderCardWidget
        self._provider_cards: dict[str, HeaderCardWidget] = {}

        # 初始加载
        self.refresh_providers()

    def refresh_providers(self) -> None:
        """刷新 Provider 列表, 从 ProviderRegistry 获取所有已注册的 Provider 并更新显示"""
        from src.core.agent.provider import ProviderRegistry

        registry: ProviderRegistry = it(ProviderRegistry)

        # 清除现有卡片 - 从 SettingCardGroup 内部布局中移除
        for card in self._provider_cards.values():
            card.setParent(None)
            card.deleteLater()
        self._provider_cards.clear()

        # 获取活跃状态
        active_provider_id: str | None = None
        active_model_id: str | None = None
        try:
            active_provider, active_model_config = registry.get_active()
            active_provider_id = active_provider.provider_id
            active_model_id = active_model_config.model_id
        except Exception:
            # NoActiveProviderError 或其他异常 - 无活跃 Provider
            pass

        # 为每个 Provider 创建卡片
        providers = registry.list_all()
        for provider in providers:
            card = self._create_provider_card(
                provider, active_provider_id, active_model_id
            )
            self.provider_group.addSettingCard(card)
            self._provider_cards[provider.provider_id] = card

    def _create_provider_card(
        self,
        provider,
        active_provider_id: str | None,
        active_model_id: str | None,
    ) -> HeaderCardWidget:
        """为单个 Provider 创建展示卡片

        Args:
            provider: Provider 实例
            active_provider_id: 当前活跃的 provider_id (可为 None)
            active_model_id: 当前活跃的 model_id (可为 None)

        Returns:
            配置好的 HeaderCardWidget
        """
        card = HeaderCardWidget(self.view)
        card.setTitle(provider.name)

        # 创建内容区域
        content_widget = QWidget(card)
        content_layout = QVBoxLayout(content_widget)
        content_layout.setContentsMargins(0, 0, 0, 0)
        content_layout.setSpacing(4)

        # API Base URL
        url_label = CaptionLabel(str(provider.api_base_url), content_widget)
        url_label.setTextColor("#666666", "#999999")
        content_layout.addWidget(url_label)

        # 模型选择 ComboBox
        model_row = QHBoxLayout()
        model_row.setContentsMargins(0, 4, 0, 0)
        model_label = BodyLabel(self.tr("模型:"), content_widget)
        model_row.addWidget(model_label)

        model_combo = ComboBox(content_widget)
        model_ids: list[str] = []
        current_index = -1
        for i, model in enumerate(provider.models):
            display = model.display_name or model.model_id
            model_combo.addItem(display)
            model_ids.append(model.model_id)
            # 如果此 provider 是活跃的且此模型是活跃模型, 记录索引
            if (
                provider.provider_id == active_provider_id
                and model.model_id == active_model_id
            ):
                current_index = i

        if current_index >= 0:
            model_combo.setCurrentIndex(current_index)
        elif provider.models:
            model_combo.setCurrentIndex(0)

        model_row.addWidget(model_combo, 1)
        content_layout.addLayout(model_row)

        # 连接 ComboBox 选择变更 → set_active
        pid = provider.provider_id
        model_combo.currentIndexChanged.connect(
            lambda idx, p=pid, ids=model_ids: self.set_active(p, ids[idx]) if 0 <= idx < len(ids) else None
        )

        card.viewLayout.addWidget(content_widget)

        # Header 右侧区域: 活跃徽章 + 删除按钮
        card.headerLayout.addStretch(1)

        # 如果是活跃 Provider, 添加高亮徽章
        if provider.provider_id == active_provider_id:
            badge = InfoBadge.success(self.tr("活跃"), parent=card)
            card.headerLayout.addWidget(badge, 0, Qt.AlignmentFlag.AlignVCenter)

        # 删除按钮
        delete_button = TransparentToolButton(FluentIcon.DELETE, card)
        delete_button.setToolTip(self.tr("删除此 Provider"))
        delete_button.setFixedSize(32, 32)
        # 使用 lambda 捕获 provider_id
        delete_button.clicked.connect(lambda checked=False, p=pid: self.remove_provider(p))
        card.headerLayout.addWidget(delete_button, 0, Qt.AlignmentFlag.AlignVCenter)

        # 存储 provider_id 到卡片属性, 方便后续操作
        card.setProperty("provider_id", provider.provider_id)

        return card

    def _on_add_provider_clicked(self) -> None:
        """处理 '添加 Provider' 按钮点击事件, 弹出表单对话框"""
        dialog = AddProviderDialog(self.window())
        if dialog.exec():
            provider_data = dialog.get_provider_data()
            self.add_provider(provider_data)

    def add_provider(self, provider_data: dict) -> None:
        """添加新的 Provider

        验证数据并调用 ProviderRegistry.register().
        成功后刷新列表, DuplicateProviderError 时显示错误通知.

        Args:
            provider_data: Provider 配置数据, 包含 provider_id, name, api_base_url,
                          api_key_ref, models 等字段
        """
        from src.core.agent.provider import ModelEntry, Provider, ProviderRegistry
        from src.core.agent.errors import DuplicateProviderError

        registry: ProviderRegistry = it(ProviderRegistry)

        # 构建 ModelEntry 列表
        model_entries = []
        for m in provider_data.get("models", []):
            model_entries.append(ModelEntry(
                model_id=m["model_id"],
                max_tokens=m["max_tokens"],
                display_name=m.get("display_name", ""),
            ))

        # 构建 Provider 实例
        try:
            provider = Provider(
                provider_id=provider_data["provider_id"],
                name=provider_data["name"],
                api_base_url=provider_data["api_base_url"],
                api_key_ref=provider_data["api_key_ref"],
                models=model_entries,
            )
        except Exception as e:
            error_bar(
                content=str(e),
                title=self.tr("数据验证失败"),
                parent=self,
            )
            return

        # 注册到 ProviderRegistry
        try:
            registry.register(provider)
        except DuplicateProviderError:
            error_bar(
                content=self.tr("provider_id 已存在"),
                title=self.tr("注册失败"),
                parent=self,
            )
            return

        # 注册成功, 刷新列表
        success_bar(
            content=self.tr(f"Provider '{provider_data['name']}' 已添加"),
            parent=self,
        )
        self.refresh_providers()

    def remove_provider(self, provider_id: str) -> None:
        """移除指定的 Provider

        调用 ProviderRegistry.unregister() 并刷新列表.

        Args:
            provider_id: 要移除的 Provider 的唯一标识
        """
        from src.core.agent.provider import ProviderRegistry

        registry: ProviderRegistry = it(ProviderRegistry)

        try:
            registry.unregister(provider_id)
        except KeyError:
            error_bar(
                content=self.tr(f"Provider '{provider_id}' 不存在"),
                title=self.tr("删除失败"),
                parent=self,
            )
            return

        success_bar(
            content=self.tr(f"Provider '{provider_id}' 已删除"),
            parent=self,
        )
        self.refresh_providers()

    def set_active(self, provider_id: str, model_id: str) -> None:
        """设置活跃的 Provider 和模型组合

        调用 ProviderRegistry.set_active() 并通过 ConfigPersistence 持久化配置.
        成功后刷新列表以更新活跃指示器.

        Args:
            provider_id: Provider 的唯一标识
            model_id: 模型的唯一标识
        """
        from src.core.agent.config_persistence import ConfigData, ConfigPersistence
        from src.core.agent.errors import ModelNotFoundError
        from src.core.agent.provider import ProviderRegistry
        from src.core.runtime.paths import PathFunc

        registry: ProviderRegistry = it(ProviderRegistry)

        try:
            registry.set_active(provider_id, model_id)
        except KeyError:
            error_bar(
                content=self.tr(f"Provider '{provider_id}' 不存在"),
                title=self.tr("设置失败"),
                parent=self,
            )
            return
        except ModelNotFoundError:
            error_bar(
                content=self.tr(f"模型 '{model_id}' 不存在于 Provider '{provider_id}' 中"),
                title=self.tr("设置失败"),
                parent=self,
            )
            return

        # 持久化配置
        try:
            path_func: PathFunc = it(PathFunc)
            config_file_path = path_func.config_dir_path / "agent_config.json"
            persistence = ConfigPersistence(config_file_path)

            # 加载现有配置并更新活跃状态
            config_data = persistence.load()
            config_data.active_provider_id = provider_id
            config_data.active_model_id = model_id

            # 同步 providers 列表到当前注册表状态
            config_data.providers = registry.list_all()

            persistence.save(config_data)
        except Exception:
            # 持久化失败不阻塞 UI 操作, 仅记录
            import logging

            logging.getLogger(__name__).warning(
                "持久化 Provider 活跃状态失败", exc_info=True
            )

        success_bar(
            content=self.tr(f"已激活: {provider_id} / {model_id}"),
            parent=self,
        )
        self.refresh_providers()
