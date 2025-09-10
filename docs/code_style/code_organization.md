# 代码组织规范

## 基本原则

1. 代码应遵循"单一职责"原则，一个类或函数只负责一项功能
2. 保持适当的代码粒度，避免过大的类或函数
3. 相关功能应组织在一起，形成清晰的模块结构
4. 高内聚低耦合，减少模块间的依赖关系
5. 代码结构应易于导航和理解

## 类内方法顺序规范

为了提高代码可读性和一致性，类中方法应按以下顺序组织：

1. **构造与析构方法**

   * `__init__`
   * `__del__`（若需要）
   * 其他特殊方法（`__str__`, `__repr__` 等）

2. **Qt 特殊方法 / 生命周期相关**

   * `setup_xxx` 初始化界面或组件方法
   * `showEvent`, `closeEvent`, `resizeEvent` 等重写的 Qt 事件处理方法
   * `paintEvent`, `resizeEvent` 等界面绘制相关方法

3. **公共方法（对外 API）**

   * 提供给其他模块调用的核心业务逻辑方法
   * 应尽量保持语义清晰，参数和返回值明确
   * 按功能相关性分组排列

4. **槽函数（Slot）**

   * 按功能分组，建议在方法名前加 `on_` 前缀
   * 可以在类内集中放置，方便查找和维护
   * 相关联的槽函数放在一起

5. **辅助方法（Helper）**

   * 用于类内部逻辑的工具性方法
   * 命名上应能体现用途
   * 如仅限类内使用，加 `_` 前缀
   * 按被调用的顺序或功能相关性排列

6. **私有方法**

   * 命名以 `_` 开头
   * 辅助公共方法或内部逻辑
   * 按功能分组排列

7. **静态方法 / 类方法**

   * `@staticmethod` 和 `@classmethod` 方法统一放在末尾
   * 工具性静态方法优先于类方法

8. **属性（Property）定义**

   * 使用 `@property` 和对应的 setter
   * 建议放在类最后，便于直观阅读
   * 相关的属性定义放在一起

### 示例
class UserProfileDialog(QDialog):
    """用户资料对话框，用于显示和编辑用户信息"""
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self.user_id = None
        self.setup_ui()
        self.load_default_settings()
        
    def __str__(self):
        return f"UserProfileDialog(user_id={self.user_id})"
    
    def setup_ui(self):
        """初始化界面组件和布局"""
        pass
        
    def showEvent(self, event):
        """重写显示事件，加载用户数据"""
        super().showEvent(event)
        self.load_user_data()
        
    # 公共方法
    def load_user_data(self):
        """加载用户数据并更新界面"""
        pass
        
    def save_user_data(self):
        """保存用户数据到数据库"""
        if self._validate_input():
            # 保存逻辑
            pass
            
    # 槽函数
    def on_submit_button_clicked(self):
        """提交按钮点击事件处理"""
        self.save_user_data()
        
    def on_cancel_button_clicked(self):
        """取消按钮点击事件处理"""
        self.reject()
        
    # 辅助方法和私有方法
    def _validate_input(self):
        """验证用户输入的有效性"""
        pass
        
    def _format_phone_number(self, number):
        """格式化电话号码"""
        pass
        
    # 静态方法
    @staticmethod
    def format_username(username: str) -> str:
        """格式化用户名（首字母大写）"""
        return username.strip().title()
        
    # 属性定义
    @property
    def is_valid(self):
        """检查当前用户数据是否有效"""
        return self.user_id is not None
        
    @property
    def user_full_name(self):
        """获取用户全名"""
        return f"{self.first_name} {self.last_name}"
## 模块组织规范

1. 相关的类和函数应放在同一个模块中
2. 一个模块的代码不宜过长，建议不超过1000行
3. 功能复杂的模块应拆分为多个子模块
4. 模块间的导入应明确，避免循环导入
5. 公共接口应在包的 `__init__.py` 中声明

## 导入语句顺序

导入语句应按以下顺序组织：

1. 标准库导入
2. 第三方库导入
3. 项目内部模块导入

每组导入之间用空行分隔，示例：
# 标准库
import os
import sys
from datetime import datetime

# 第三方库
import requests
from PyQt5.QtWidgets import QDialog, QPushButton

# 项目内部模块
from .user_manager import UserManager
from ..utils.validation import validate_email