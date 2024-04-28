# -*- coding: utf-8 -*-
from PySide6.QtCore import QOperatingSystemVersion, QObject

from src.Core.CreateScript.ConfigModel import ScriptType, Config


class CreateScript:
    """
    ## 创建启动脚本:
    传入配置文件和脚本类型生成脚本, 按需实例化
    """

    def __init__(self, config: dict, scriptType: ScriptType) -> None:
        """
        ## 验证传入的参数:
        如果传入参数有问题,则会返回 false 并附带一个错误信息用于提示

        ### 参数:
            - config: 脚本所需的变量
            - scriptType: 脚本类型
        """
        self.config = Config(**config)
        self.scriptType = self._verifySystemSupports(scriptType)

    @staticmethod
    def _verifySystemSupports(scriptType: ScriptType) -> ScriptType:
        """验证系统是否支持脚本

        Windows 平台不支持 sh脚本
        Linux 平台不支持 bat, ps1 脚本
        """
        systemType = QOperatingSystemVersion.currentType()

        if scriptType == ScriptType.BAT or scriptType == ScriptType.PS1:
            # 判断脚本类型
            if systemType == QOperatingSystemVersion.OSType.Windows:
                # 如果该类型脚本系统支持则验证成功,反之报错
                return scriptType
            raise TypeError(
                QObject.tr(
                    "Currently, the system does not support "
                    "the creation of bat, ps1 scripts"
                )
            )
        else:
            if systemType == QOperatingSystemVersion.OSType.Windows:
                # QOperatingSystemVersion 没有 Linux Type
                raise TypeError(
                    QObject.tr(".sh scripts are not supported by the current system")
                )
            return scriptType


if __name__ == '__main__':
    create = CreateScript({}, ScriptType.PS1)
    print(create.scriptType)


