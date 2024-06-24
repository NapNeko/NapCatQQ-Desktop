# -*- coding: utf-8 -*-
from qfluentwidgets import MessageBoxBase, TitleLabel, BodyLabel


class EULAMessageBox(MessageBoxBase):
    """
    ## 最终用户许可协议
    """

    def __init__(self, parent=None):
        super().__init__(parent)

        # 创建控件
        self.titleLabel = TitleLabel(self.tr("EULA"), self)
        self.contentsLabel = BodyLabel(
            self.tr(
                "This project is for learning PySide6 only; do not use it for illegal activities.\n\n"
                "All consequences arising from the use of this project are the sole responsibility of "
                "the user and are not related to the author or contributors of this project.\n\n"
                "This project is provided 'as is' without any express or implied warranties, including "
                "but not limited to merchantability, fitness for a particular purpose, and non-infringement.\n\n"
                "To the maximum extent permitted by law, the author and contributors are not liable for any "
                "direct, indirect, incidental, special, punitive, or consequential damages arising from the use "
                "of this project, including but not limited to data loss, profit loss, or business interruption.\n\n"
                "This disclaimer also applies to users who obtain and use this project through GitHub Actions or "
                "packaged versions in releases.\n\n"
                "Before using this project, ensure that you have read and fully understood this disclaimer. "
                "If you do not agree with any of the terms in this disclaimer, do not use this project.\n"
            ),
            self
        )

        # 设置控件
        self.contentsLabel.setWordWrap(True)
        self.yesButton.setText(self.tr("I have carefully read and agree to the above terms"))
        self.cancelButton.setText(self.tr("Exit the software"))

        self.setMinimumSize(400, 400)
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.contentsLabel)
