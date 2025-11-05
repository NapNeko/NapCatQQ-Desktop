# -*- coding: utf-8 -*-

"""
这个模块包含的是 NapCatQQ Desktop 的配置项

每个配置项都对应一个类变量, 变量名即为配置项的键名
配置项的类型可以是以下几种:
- ConfigItem: 基础配置项, 支持 str, int, float, bool 等基本类型
- OptionsConfigItem: 选项配置项, 支持枚举类型和指定选项列表
- RangeConfigItem: 范围配置项, 支持数值类型, 并且有最小值和最大值限制
- ColorConfigItem: 颜色配置项, 支持颜色字符串, 如 "#RRGGBB" 或 "#AARRGGBB"
- 其他类型可以参考 qfluentwidgets.common 模块

每个配置项都可以指定以下参数:
- group: 配置项所属的组名, 用于分类显示
- name: 配置项的键名, 用于存储和读取配置文件
- default: 配置项的默认值, 如果配置文件中没有该项, 则使用默认值
- validator: 配置项的验证器, 用于验证配置项的值是否合法
- serializer: 配置项的序列化器, 用于将配置项的值转换为字符串存储到配置文件中, 或从字符串读取并转换为配置项的值
- restart: 是否需要重启应用程序才能生效, 如果为 True, 则在修改该配置项后会发出 app_restart_signal 信号
- 其他参数可以参考 qfluentwidgets.common 模块

注意:
- 配置文件默认存储在用户的配置目录下, 可以通过 PathFunc().config_path 获取
- 修改配置项后, 需要调用 cfg.set(item, value) 方法来设置新的值, 这样才能触发验证和序列化
- 如果配置项的值不合法, 则会抛出 ValueError 异常
- 如果配置文件不存在或格式错误, 则会使用默认配置
- 如果程序更新后, 有新增的配置项, 则会自动添加默认值(此方法在 Config.load 方法中实现, 而非 QConfig.load)
- 具体使用方法可以参考 qfluentwidgets.common.QConfig 类的文档
- 与 config_model.py 区别在于, config_model.py 主要用于定义与机器人相关的配置模型, 而此模块主要用于定义应用程序的整体配置项
"""

# 标准库导入
import inspect
import json
import platform
import time
from pathlib import Path
from typing import Self

# 第三方库导入
from qfluentwidgets.common import (
    BoolValidator,
    RangeConfigItem,
    RangeValidator,
    qconfig,
    ColorConfigItem,
    ConfigItem,
    ConfigSerializer,
    EnumSerializer,
    OptionsConfigItem,
    OptionsValidator,
    QConfig,
    Theme,
)
from qfluentwidgets.common.exception_handler import exceptionHandler
from PySide6.QtCore import QLocale, Signal

# 项目内模块导入
from src.core.config.config_enum import CloseActionEnum, Language
from src.core.utils.logger import logger
from src.core.utils.path_func import PathFunc

__version__ = "v1.7.7"


class LanguageSerializer(ConfigSerializer):
    """语言序列化"""

    def serialize(self, language):
        return language.value.name() if language != Language.AUTO else "Auto"

    def deserialize(self, value: str):
        return Language(QLocale(value)) if value != "Auto" else Language.AUTO


class Config(QConfig):
    """程序配置"""

    # 信号
    app_restart_signal = Signal()

    # 信息项
    napcat_desktop_version = ConfigItem(group="Info", name="NCDVersion", default="")
    start_time = ConfigItem(group="Info", name="StartTime", default="")
    system_type = ConfigItem(group="Info", name="SystemType", default="")
    platform_type = ConfigItem(group="Info", name="PlatformType", default="")
    main_window = ConfigItem(group="Info", name="MainWindow", default=False, validator=BoolValidator())
    elua_accepted = ConfigItem(group="Info", name="EulaAccepted", default=False, validator=BoolValidator())

    # 个性化项目
    language = OptionsConfigItem(
        group="Personalize",
        name="Language",
        default=Language.AUTO,
        validator=OptionsValidator(Language),
        serializer=LanguageSerializer(),
        restart=True,
    )
    theme_mode = OptionsConfigItem(
        group="Personalize",
        name="ThemeMode",
        default=Theme.AUTO,
        validator=OptionsValidator(Theme),
        serializer=EnumSerializer(Theme),
    )
    theme_color = ColorConfigItem(
        group="Personalize",
        name="ThemeColor",
        default="#009faa",
    )
    dpi_scale = OptionsConfigItem(
        group="Personalized",
        name="DpiScale",
        default="Auto",
        validator=OptionsValidator([1, 1.25, 1.5, 1.75, 2, "Auto"]),
        restart=True,
    )
    close_button_action = OptionsConfigItem(
        group="General",
        name="CloseBtnAction",
        default=CloseActionEnum.CLOSE,
        validator=OptionsValidator(list(CloseActionEnum)),
        serializer=EnumSerializer(CloseActionEnum),
    )
    window_opacity = RangeConfigItem(
        group="Personalize",
        name="WindowOpacity",
        default=100,
        validator=RangeValidator(10, 100),
    )
    title_tab_bar = ConfigItem(
        group="Personalize",
        name="TitleTabBar",
        default=False,
        validator=BoolValidator(),
    )
    title_tab_bar_movable = ConfigItem(
        group="Personalize",
        name="TitleTabBarIsMovable",
        default=False,
        validator=BoolValidator(),
    )
    title_tab_bar_scrollable = ConfigItem(
        group="Personalize",
        name="TitleTabBarIsScrollable",
        default=False,
        validator=BoolValidator(),
    )
    title_tab_bar_shadow = ConfigItem(
        group="Personalize",
        name="TitleTabBarIsShadow",
        default=False,
        validator=BoolValidator(),
    )
    title_tab_bar_close_mode = OptionsConfigItem(
        group="Personalize",
        name="TitleTabBarCloseButton",
        default="悬停显示",
        validator=OptionsValidator(["始终显示", "悬停显示", "永不显示"]),
        restart=True,
    )
    title_tab_bar_min_width = RangeConfigItem(
        group="Personalize",
        name="TitleTabBarMinWidth",
        default=64,
        validator=RangeValidator(32, 64),
    )
    title_tab_bar_max_width = RangeConfigItem(
        group="Personalize",
        name="TitleTabBarMaxWidth",
        default=135,
        validator=RangeValidator(64, 200),
    )

    # 事件项
    bot_offline_email_notice = ConfigItem(
        group="Event", name="BotOfflineEmailNotice", default=False, validator=BoolValidator()
    )
    bot_offline_web_hook_notice = ConfigItem(
        group="Event", name="BotOfflineWebHookNotice", default=False, validator=BoolValidator()
    )

    # 邮件项
    email_receiver = ConfigItem(group="Email", name="EmailReceiver", default="")
    email_sender = ConfigItem(group="Email", name="EmailSender", default="")
    email_token = ConfigItem(group="Email", name="EmailToken", default="")
    email_stmp_server = ConfigItem(group="Email", name="EmailStmpServer", default="")
    email_stmp_port = RangeConfigItem(
        group="Email",
        name="EmailStmpPort",
        default=465,
        validator=RangeValidator(1, 65535),
    )
    email_encryption = OptionsConfigItem(
        group="Email",
        name="EmailEncryption",
        default="SSL",
        validator=OptionsValidator(["SSL", "TLS", "无加密"]),
    )

    # WebHook 项
    web_hook_url = ConfigItem(group="WebHook", name="WebHookUrl", default="")
    web_hook_secret = ConfigItem(group="WebHook", name="WebHookSecret", default="")
    web_hook_json = ConfigItem(group="WebHook", name="WebHookJson", default="")

    def __init__(self):
        super().__init__()

        # 修改 Config 初始化
        self.fontFamilies.restart = True

        # 连接重启提示信号
        self.appRestartSig.connect(self.slot_restart_tips)

    def slot_restart_tips(self) -> None:
        """接收重启提示信号, 发出应用重启提示"""
        # 项目内模块导入
        from src.ui.components.info_bar import success_bar

        success_bar(self.tr("配置在重启后生效"))

    @exceptionHandler()
    def load(self, file: str | Path = None, config: Self = None) -> None:
        """重写加载配置

        能够在程序更新后,如果有新增的配置项,能自动添加默认值

        此处的 file 和 config 参数,与 QConfig.load 的参数意义相同
        但优先级为 config > file > self._cfg

        Args:
            file (str | Path, optional): 配置文件路径. Defaults to None.
            config (Self@Config, optional): 需要加载的配置类. Defaults to None.
        """

        # 处理 config 和 file 参数
        if isinstance(config, QConfig):
            self._cfg = config
            self._cfg.themeChanged.connect(self.themeChanged)

        if isinstance(file, (str, Path)):
            self._cfg.file = Path(file)

        # 加载配置文件
        try:
            with open(self._cfg.file, "r", encoding="utf-8") as f:
                cfg = json.load(f)
        except (FileNotFoundError, json.JSONDecodeError) as e:
            logger.error(f"配置文件加载失败: {e.__class__.__name__}: {e.msg}, 使用默认配置")
            cfg = {}

        # 获取配置项
        items = {
            item.key: item for _, item in inspect.getmembers(self._cfg.__class__, lambda x: isinstance(x, ConfigItem))
        }

        # 展平配置字典
        def flatten(d, parent_key: str = ""):
            """展平嵌套字典, 键用点连接"""
            for k, v in d.items():
                key = f"{parent_key}.{k}" if parent_key else k
                if isinstance(v, dict):
                    yield from flatten(v, key)
                else:
                    yield key, v

        flat_cfg = dict(flatten(cfg))

        # 更新配置项
        for k, v in flat_cfg.items():
            if k in items:
                items[k].deserializeFrom(v)

        # 补齐默认值
        for item in items.values():
            if not getattr(item, "value", None):
                item.value = item.default

        # 应用主题
        self.theme = self.get(self._cfg.themeMode)


cfg = Config()
qconfig.load(PathFunc().config_path, cfg)
cfg.set(cfg.start_time, time.time(), True)
cfg.set(cfg.napcat_desktop_version, __version__, True)
cfg.set(cfg.system_type, platform.system(), True)
cfg.set(cfg.platform_type, platform.machine(), True)
