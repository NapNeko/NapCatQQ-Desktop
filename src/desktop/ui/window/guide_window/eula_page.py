# -*- coding: utf-8 -*-
from __future__ import annotations

# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from markdown import markdown
from qfluentwidgets import PrimaryPushButton, PushButton, TitleLabel
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config import cfg
from src.ui.components.code_editor.exhibit import UpdateLogExhibit

if TYPE_CHECKING:

    # 项目内模块导入
    from src.ui.window.guide_window.guide_window import GuideWindow

EULA_CONTENT = """
## 1. 许可与使用范围

本软件 (NapCatQQ-Desktop) 仅为方便用户使用 NapCatQQ 而设计, 仅限于学习 PySide6 及相关技术用途. 任何用户不得将本软件用于任何违法犯罪行为.

为防止滥用和潜在的违规操作, 本软件将技术上限制用户最多同时登录 4 个账号. 任何试图绕过此限制的行为均被视为违反本协议.

## 2. 免责声明

使用本软件所产生的一切后果, 包括但不限于法律风险, 经济损失或其他任何形式的损害, 均由使用者自行承担. 本软件的作者及贡献者不承担任何形式的责任.

## 3. 无担保声明

本软件以 "现状" 提供, 作者及贡献者未对本软件作任何明示或暗示的担保, 包括但不限于适销性, 特定用途适用性及不侵犯第三方权利等. 用户需自行承担使用本软件的所有风险.

## 4. 责任限制

在法律允许的最大范围内, 作者及贡献者不对因使用或无法使用本软件而导致的任何直接, 间接, 附带, 特殊, 惩罚性或后果性损害负责, 包括但不限于数据丢失, 利润损失或业务中断等.

## 5. 衍生代码责任豁免

**即使您通过复刻 (Fork) 本代码仓库并修改源代码的方式绕过了本软件内置的任何限制 (包括但不限于账号登录数量限制), 由此产生的一切后果及法律责任仍由您自行承担, 与本项目原仓库的维护者及贡献者无关. 本协议条款对衍生代码同样具有约束和警示意义.**

## 6. 特别声明

NapCatQQ-Desktop 仅为方便使用 NapCatQQ 的工具, **并非用于违法犯罪的工具**. 如用户通过包括但不限于多开, 技术手段绕过登录限制等方法, 利用本软件从事任何违法犯罪活动, 本软件的作者及贡献者不承担任何连带责任.

## 7. 适用范围

本协议适用于通过 GitHub Actions, Releases 中的打包编译版本, 或任何其他方式获取并使用本软件的用户.

## 8. 用户义务

在使用本软件前, 请确保您已仔细阅读并完全理解本协议的所有条款. **您一旦使用本软件, 即表示您同意并承诺遵守上述关于最多同时登录 4 个账号的技术限制, 且不会将其用于任何非法目的.** 如您不同意本协议中的任何内容, 请立即停止使用本软件.
"""


class EulaPage(QWidget):
    """欢迎页面"""

    def __init__(self, parent: "GuideWindow") -> None:
        super().__init__(parent)

        # 创建控件
        self.title_label = TitleLabel("最终用户许可协议 (EULA)", self)
        self.eula_exhibit = UpdateLogExhibit(self)
        self.accept_button = PrimaryPushButton("同意", self)
        self.reject_button = PushButton("不同意", self)

        # 设置控件
        css_style = "<style>pre { white-space: pre-wrap; }</style>"
        self.eula_exhibit.setHtml(css_style + markdown(EULA_CONTENT, extensions=["nl2br"]))

        # 设置布局
        self.h_box_layout = QHBoxLayout()
        self.h_box_layout.setContentsMargins(0, 0, 0, 0)
        self.h_box_layout.setSpacing(8)
        self.h_box_layout.addWidget(self.accept_button)
        self.h_box_layout.addWidget(self.reject_button)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(36, 24, 24, 24)
        self.v_box_layout.addWidget(self.title_label)
        self.v_box_layout.addWidget(self.eula_exhibit)
        self.v_box_layout.addLayout(self.h_box_layout)

        # 信号连接
        self.accept_button.clicked.connect(self.slot_accept)
        self.reject_button.clicked.connect(self.slot_reject)

    # ==================== 槽函数 ====================
    def slot_accept(self) -> None:
        """用户同意EULA"""
        # 项目内模块导入
        from src.ui.window.guide_window.guide_window import GuideWindow

        cfg.set(cfg.elua_accepted, True)

        # 如果主窗口打开过，则关闭向导窗口, 否则进入下一个页面
        if cfg.get(cfg.main_window):
            it(GuideWindow).close()
        else:
            it(GuideWindow).on_next_page()

    def slot_reject(self) -> None:
        """用户不同意EULA"""
        # 标准库导入
        from sys import exit

        exit(0)
