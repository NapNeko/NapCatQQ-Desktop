# NapCatQQ Desktop 项目命名规范

## 一、通用规则

1. 遵循 Python 官方 [PEP 8](https://www.python.org/dev/peps/pep-0008/) 规范
2. 保持代码风格统一，避免在同一项目中混用驼峰体与下划线命名法
3. 所有命名应语义清晰、可读性强，避免使用缩写和无意义的字母组合

## 二、文件与目录命名

* 文件名、目录名统一采用 `snake_case`（全小写 + 下划线）
* 示例：

  ```
  main_window.py
  user_profile/
  database_connector.py
  ```

## 三、类命名

* 使用 **PascalCase**（大驼峰命名）
* 示例：

  ```python
  class MainWindow(QMainWindow):
      pass

  class UserProfileDialog(QDialog):
      pass

  class DatabaseConnectionManager:
      pass
  ```

## 四、函数与方法命名

* 普通函数与方法统一使用 `snake_case`

* 示例：

  ```python
  def load_user_data():
      pass

  def refresh_table_view():
      pass

  def calculate_total_amount():
      pass
  ```

* 槽函数推荐使用 `on_xxx` 格式命名

* 示例：

  ```python
  def on_login_button_clicked():
      pass

  def on_text_changed():
      pass

  def on_selection_changed():
      pass
  ```

## 五、变量命名

* 局部变量与实例变量统一使用 `snake_case`

* 示例：

  ```python
  user_id = 123
  current_index = 0
  is_connected = True
  ```

* 常量使用 `UPPER_SNAKE_CASE`（全大写 + 下划线）

* 示例：

  ```python
  DEFAULT_TIMEOUT = 30
  API_BASE_URL = "https://api.example.com"
  MAX_RETRY_COUNT = 3
  ```

* Qt UI 组件实例应体现控件类型

* 示例：

  ```python
  submit_button = QPushButton()
  username_line_edit = QLineEdit()
  result_table_widget = QTableWidget()
  ```

## 六、信号命名

* 自定义信号使用 `snake_case_signal`
* 示例：

  ```python
  class DataManager(QObject):
      data_loaded_signal = Signal()
      user_logged_in_signal = Signal(str)  # 用户名
      connection_status_changed_signal = Signal(bool)  # 连接状态
  ```

## 七、命名冲突与前缀处理

* 避免使用内置关键字，可添加后缀区分

* 示例：

  ```python
  # 避免
  class = None  # ❌ 关键字冲突

  # 推荐
  widget_class = "QPushButton"  # ✅
  user_type = "admin"  # ✅
  ```

* 私有方法添加 `_` 前缀

* 示例：

  ```python
  def _internal_helper_method(self):
      pass

  def _validate_input_data(self, data):
      pass
  ```

## 八、资源文件命名规范

* 所有资源文件放在统一的 `resources/` 或 `assets/` 目录

* 可按类型进一步分类：

  ```
  resources/images/      # 图片
  resources/fonts/       # 字体
  resources/styles/      # QSS 样式
  resources/templates/   # UI 模板或其他可复用文件
  ```

* 文件名统一使用 `snake_case`，语义清晰

* 示例：

  ```
  logo.png
  background_home.jpg
  roboto_regular.ttf
  iconfont.ttf
  main_window.qss
  dark_theme.qss
  user_dialog.ui
  report_template.ui
  ```

* 避免空格和特殊字符，推荐使用下划线 `_` 连接单词

* 对于不同分辨率或 DPI 的图片，可以在文件名中标注尺寸

  ```
  icon_24x24.png
  banner_1920x1080.jpg
  ```

* 对频繁更新或版本迭代的资源文件，可在文件名中加版本号或日期

  ```
  logo_v2.png
  background_202509.ui
  ```

* 代码中引用资源时，使用统一的路径变量或资源管理类，避免硬编码

## 九、文档与示例

* 建立 `docs/naming_convention.md` 文档
* 在 README 中注明开发规范要求

## 十、迁移计划

1. 逐步重构现有代码和资源文件，避免大规模破坏性修改
2. 新代码与新资源必须符合规范，代码审查时重点检查命名一致性

---

说明：本文档是项目代码质量、可维护性和资源管理的重要保障，请所有开发人员严格遵守。
