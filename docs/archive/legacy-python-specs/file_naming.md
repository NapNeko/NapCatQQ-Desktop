# 文件与目录命名规范

## 基本原则

1. 所有文件名和目录名统一采用 `snake_case`（全小写 + 下划线）
2. 命名应准确反映文件或目录的内容和用途
3. 避免使用缩写，除非是广为人知的技术术语
4. 文件名应包含适当的扩展名，清晰标识文件类型
5. 避免使用空格和特殊字符

## 目录命名

* 使用 `snake_case` 命名目录
* 按功能或模块划分目录结构
* 示例：

  ```
  user_profile/
  message_center/
  database_connector/
  ui_components/
  utils/
  ```

* 公共组件或工具函数放在 `common/` 或 `utils/` 目录
* 资源文件放在 `resources/` 或 `assets/` 目录
* 测试代码放在 `tests/` 目录，与源代码结构保持一致

## 文件命名

### Python 文件

* 使用 `snake_case.py` 命名
* 单个类的实现文件可与类名对应（转为snake_case）
* 示例：

  ```
  main_window.py
  user_profile_dialog.py
  database_connector.py
  ```

* 工具类集合文件可命名为 `xxx_utils.py`
* 配置相关文件可命名为 `xxx_config.py`

### UI 文件

* Qt Designer UI 文件使用 `snake_case.ui` 命名
* 示例：

  ```
  main_window.ui
  user_login_dialog.ui
  message_bubble.ui
  ```

### 样式文件

* QSS 样式文件使用 `snake_case.qss` 命名
* 示例：

  ```
  main_window.qss
  dark_theme.qss
  ```

## 特殊文件

* 包初始化文件统一命名为 `__init__.py`
* 主程序入口推荐命名为 `main.py` 或 `app.py`
* 版本信息文件推荐命名为 `version.py`
* 常量定义文件推荐命名为 `constants.py`

## 命名示例

完整的目录结构示例：
napcat_qq/
├── main.py
├── version.py
├── constants.py
├── main_window.py
├── main_window.ui
├── user_profile/
│   ├── __init__.py
│   ├── user_profile_dialog.py
│   ├── user_profile_dialog.ui
│   └── user_manager.py
├── message_center/
│   ├── __init__.py
│   ├── message_list.py
│   ├── message_item.py
│   └── message_utils.py
├── resources/
│   ├── images/
│   └── styles/
└── tests/
    ├── test_user_profile.py
    └── test_message_center.py