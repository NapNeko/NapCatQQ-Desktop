# NapCatQQ-Desktop 测试文档

本文档介绍如何使用 pytest 测试框架对 NapCatQQ-Desktop 项目进行测试。

## 测试框架概述

本项目使用 pytest 作为测试框架，配合以下工具：
- **pytest**: 主测试框架
- **pytest-qt**: 用于 PySide6/Qt GUI 组件测试
- **pytest-cov**: 用于生成代码覆盖率报告
- **pytest-mock**: 用于模拟外部依赖

## 当前测试状态

### ✅ 162 个测试全部通过
```bash
$ pytest tests/ -v
============================== 162 passed in 1.46s ===============================
```

### 测试分布
- **配置模块**: 35 个测试
- **工具模块**: 98 个测试 🆕
- **网络模块**: 23 个测试
- **示例测试**: 6 个测试

## 目录结构

```
tests/
├── __init__.py           # 测试包初始化文件
├── conftest.py           # pytest 配置和共享 fixtures
├── test_example.py       # 示例测试（验证框架工作）
├── test_config/          # 配置模块测试
│   ├── __init__.py
│   ├── test_config_enum.py         # 配置枚举测试 (6 tests)
│   ├── test_config_model_pydantic.py  # Pydantic 模型测试 (15 tests)
│   └── test_operate_config.py      # 配置文件读写测试 (14 tests)
├── test_utils/           # 工具模块测试
│   ├── __init__.py
│   ├── test_singleton.py           # 单例模式测试 (4 tests)
│   ├── test_logger_enum.py         # 日志枚举测试 (7 tests)
│   ├── test_logger_data.py         # 日志数据模型测试 (12 tests)
│   ├── test_mutex.py               # 互斥锁测试 (8 tests)
│   └── test_string_utils.py        # 字符串工具测试 (17 tests)
└── test_network/         # 网络模块测试
    ├── __init__.py
    ├── test_urls.py                # URL 常量测试 (5 tests)
    ├── test_email.py               # 邮件通知测试 (11 tests)
    └── test_webhook.py             # Webhook 通知测试 (12 tests)
```

## 环境准备

### 1. 安装依赖

确保已安装项目的所有依赖：

```bash
pip install -r requirements.txt
```

### 2. 安装测试依赖

测试依赖已包含在 `pyproject.toml` 和 `requirements.txt` 中：
- pytest >= 8.0.0
- pytest-qt >= 4.4.0
- pytest-cov >= 6.0.0

## 运行测试

### 🚀 运行所有测试（推荐，99 个测试）

```bash
# 简单命令 - 运行所有测试（推荐）
pytest tests/test_config/ tests/test_utils/ tests/test_network/ tests/test_example.py -v

# 不显示详细输出
pytest tests/test_config/ tests/test_utils/ tests/test_network/ tests/test_example.py

# 生成覆盖率报告
pytest tests/test_config/ tests/test_utils/ tests/test_network/ tests/test_example.py --cov=src --cov-report=html
```

**注意**：模板测试文件（以 `.template` 结尾）包含需要 GUI 依赖的示例代码，pytest 不会自动收集它们。

### 运行特定模块

```bash
# 配置模块测试 (35 tests)
pytest tests/test_config/

# 工具模块测试 (35 tests)
pytest tests/test_utils/

# 网络模块测试 (23 tests)
pytest tests/test_network/

# 示例测试 (6 tests)
pytest tests/test_example.py
```

### 运行特定测试文件

```bash
# 配置枚举测试
pytest tests/test_config/test_config_enum.py

# 单例模式测试
pytest tests/test_utils/test_singleton.py

# 邮件通知测试
pytest tests/test_network/test_email.py
```

### 运行特定测试类或方法

```bash
pytest tests/test_example.py::TestBasicMath::test_add_positive_numbers
```

### 详细输出

```bash
pytest -v
```

### 运行标记的测试

本项目使用以下测试标记：
- `unit`: 单元测试
- `integration`: 集成测试
- `gui`: GUI 测试
- `slow`: 运行较慢的测试

运行特定标记的测试：

```bash
# 只运行单元测试
pytest -m unit

# 排除 GUI 测试
pytest -m "not gui"

# 排除慢速测试
pytest -m "not slow"
```

## 代码覆盖率

### 生成覆盖率报告

```bash
# 在终端显示覆盖率
pytest --cov=src --cov-report=term-missing

# 生成 HTML 报告
pytest --cov=src --cov-report=html

# 生成 XML 报告（用于 CI/CD）
pytest --cov=src --cov-report=xml
```

### 查看 HTML 覆盖率报告

生成 HTML 报告后，在浏览器中打开：

```bash
# Linux/Mac
open htmlcov/index.html

# Windows
start htmlcov/index.html
```

## 编写测试

### 基本测试结构

```python
# -*- coding: utf-8 -*-
"""测试模块描述"""

import pytest
from src.core.config.config_model import BotConfig

class TestBotConfig:
    """测试 BotConfig 类"""

    def test_valid_config(self):
        """测试有效配置"""
        config = BotConfig(
            name="TestBot",
            QQID="123456789",
            autoRestartSchedule=AutoRestartScheduleConfig()
        )
        assert config.name == "TestBot"
        assert config.QQID == 123456789
```

### 使用 Fixtures

在 `conftest.py` 中定义的 fixtures 可在所有测试中使用：

```python
def test_with_qapp(qapp):
    """使用 QApplication fixture"""
    assert qapp is not None

def test_with_mock_config(mock_config_data):
    """使用模拟配置数据"""
    assert "qq" in mock_config_data
```

### GUI 测试示例

```python
import pytest
from PySide6.QtWidgets import QWidget

@pytest.mark.gui
class TestMessageBox:
    """测试消息框组件"""

    def test_creation(self, qapp, qtbot):
        """测试组件创建"""
        parent = QWidget()
        qtbot.addWidget(parent)
        
        # 创建和测试组件
        widget = SomeWidget(parent)
        qtbot.addWidget(widget)
        
        # 使用 qtbot 模拟用户交互
        qtbot.keyClicks(widget.input_field, "test input")
        assert widget.input_field.text() == "test input"
```

## 测试最佳实践

1. **测试命名**：测试文件以 `test_` 开头，测试函数以 `test_` 开头
2. **测试隔离**：每个测试应该独立，不依赖其他测试的执行顺序
3. **使用 fixtures**：利用 fixtures 复用测试设置代码
4. **清晰的断言**：使用明确的断言信息，便于调试
5. **标记测试**：使用 pytest 标记对测试进行分类
6. **文档字符串**：为测试类和方法添加清晰的文档字符串

## 常见问题

### Q: 运行测试时找不到模块？

A: 确保在项目根目录运行 pytest，或者检查 `sys.path` 设置。

### Q: GUI 测试失败？

A: 确保已安装 PySide6 和 pytest-qt，GUI 测试需要 X server 环境（Linux）或图形界面。

### Q: 覆盖率报告显示 0%？

A: 检查是否正确导入了被测试的模块，确保测试实际执行了代码路径。

## CI/CD 集成

测试可以在 CI/CD 流程中自动运行。参考 `.github/workflows/test.yml`（待创建）了解详情。

## 贡献指南

提交代码时，请确保：
1. 新功能包含对应的测试
2. 所有测试通过
3. 代码覆盖率不降低
4. 遵循项目的代码风格

## 参考资料

- [pytest 官方文档](https://docs.pytest.org/)
- [pytest-qt 文档](https://pytest-qt.readthedocs.io/)
- [pytest-cov 文档](https://pytest-cov.readthedocs.io/)
- [PySide6 测试指南](https://doc.qt.io/qtforpython/)
