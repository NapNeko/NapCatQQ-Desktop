# 代码命名规范

## 类命名

* 使用 **PascalCase**（大驼峰命名）
* 类名应准确描述其职责，使用名词或名词短语
* 示例：

  ```python
  class MainWindow(QMainWindow):
      pass

  class UserProfileDialog(QDialog):
      pass

  class DatabaseConnectionManager:
      pass
  ```

## 函数与方法命名

* 普通函数与方法统一使用 `snake_case`
* 函数名应体现其功能，使用动词或动词短语
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
* 槽函数应体现触发源和事件类型
* 示例：

  ```python
  def on_login_button_clicked():
      pass

  def on_text_changed():
      pass

  def on_selection_changed():
      pass
  ```

## 变量命名

* 局部变量与实例变量统一使用 `snake_case`
* 变量名应清晰表达其含义和用途
* 示例：

  ```python
  user_id = 123
  current_index = 0
  is_connected = True
  ```

* 常量使用 `UPPER_SNAKE_CASE`（全大写 + 下划线）
* 常量应在模块级别定义
* 示例：

  ```python
  DEFAULT_TIMEOUT = 30
  API_BASE_URL = "https://api.example.com"
  MAX_RETRY_COUNT = 3
  ```

* Qt UI 组件实例应体现控件类型
* 命名格式：`功能_控件类型`
* 示例：

  ```python
  submit_button = QPushButton()
  username_line_edit = QLineEdit()
  result_table_widget = QTableWidget()
  ```

## 信号命名

* 自定义信号使用 `snake_case_signal`
* 信号名应体现其传递的事件或状态变化
* 示例：

  ```python
  class DataManager(QObject):
      data_loaded_signal = Signal()
      user_logged_in_signal = Signal(str)  # 用户名
      connection_status_changed_signal = Signal(bool)  # 连接状态
  ```

## 命名冲突与前缀处理

* 避免使用内置关键字，可添加后缀区分
* 示例：

  ```python
  # 避免
  class = None  # ❌ 关键字冲突

  # 推荐
  widget_class = "QPushButton"  # ✅
  user_type = "admin"  # ✅
  ```

* 私有方法和变量添加 `_` 前缀
* 示例：

  ```python
  def _internal_helper_method(self):
      pass

  def _validate_input_data(self, data):
      pass
  ```

* 保护成员添加 `_` 前缀，供子类访问
* 示例：

  ```python
  class BaseComponent:
      def __init__(self):
          self._configuration = {}  # 保护成员，供子类使用
  ```
