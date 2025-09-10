# 资源文件管理规范

## 资源目录结构

所有资源文件应放在统一的资源目录中，建议采用以下结构：
resources/
├── images/      # 图片资源
│   ├── icons/   # 图标
│   ├── logos/   # 标志
│   ├── backgrounds/ # 背景图片
│   └── avatars/ # 头像
├── fonts/       # 字体文件
├── styles/      # QSS 样式文件
├── templates/   # UI 模板或其他可复用文件
└── translations/ # 翻译文件
## 命名规范

1. 所有资源文件统一使用 `snake_case` 命名
2. 文件名应语义清晰，准确描述资源内容
3. 避免使用空格和特殊字符，使用下划线 `_` 连接单词
4. 资源类型可在文件名中体现（如适用）

## 图片资源

1. 图片文件命名格式：`描述[_尺寸][_状态].扩展名`
2. 示例：

  ```
  logo.png
  home_icon.png
  settings_icon_hover.png
  background_main.jpg
  avatar_default.png
  ```

3. 对于不同分辨率或DPI的图片，在文件名中标注尺寸：

  ```
  icon_24x24.png
  icon_48x48.png
  banner_1920x1080.jpg
  ```

4. 对于不同状态的图片（正常、悬停、选中、禁用），在文件名中标注状态：

  ```
  button_submit_normal.png
  button_submit_hover.png
  button_submit_pressed.png
  button_submit_disabled.png
  ```

5. 图片格式选择：
   - 图标和简单图形优先使用SVG格式
   - 照片和复杂图像使用JPG格式
   - 需要透明背景的图片使用PNG格式

## 字体资源

1. 字体文件使用其原始名称，保持一致性
2. 可在文件名中包含字体样式信息：

  ```
  roboto_regular.ttf
  roboto_bold.ttf
  microsoft_yahei_light.ttf
  ```

3. 字体文件应放在 `resources/fonts/` 目录下

## 样式文件

1. QSS样式文件使用 `snake_case.qss` 命名
2. 主题相关样式文件可包含主题名称：

  ```
  main_window.qss
  dark_theme.qss
  light_theme.qss
  message_bubble.qss
  ```

3. 通用样式可命名为 `common.qss` 或 `global.qss`

## 资源版本管理

1. 对频繁更新或版本迭代的资源文件，可在文件名中加版本号：

  ```
  logo_v2.png
  theme_v3.qss
  ```

2. 过时的资源文件应及时清理，避免冗余

## 资源使用规范

1. 代码中引用资源时，使用统一的路径变量或资源管理类，避免硬编码
2. 示例：

  ```python
  # 推荐
  from resources.resource_manager import ResourceManager
  
  icon = QIcon(ResourceManager.get_image_path("home_icon.png"))
  
  # 不推荐
  icon = QIcon("resources/images/icons/home_icon.png")  # ❌ 硬编码路径
  ```

3. 大型资源应考虑延迟加载，提高应用启动速度
4. 图片资源应根据需要进行压缩，平衡质量和文件大小

## 资源管理类示例
# resources/resource_manager.py
import os
from pathlib import Path

class ResourceManager:
    """资源管理类，统一管理资源文件路径"""
    
    # 基础资源路径
    _BASE_RESOURCE_PATH = Path(__file__).parent.parent / "resources"
    
    @classmethod
    def get_image_path(cls, image_name):
        """获取图片资源路径"""
        return str(cls._BASE_RESOURCE_PATH / "images" / image_name)
    
    @classmethod
    def get_font_path(cls, font_name):
        """获取字体资源路径"""
        return str(cls._BASE_RESOURCE_PATH / "fonts" / font_name)
    
    @classmethod
    def get_style_path(cls, style_name):
        """获取样式文件路径"""
        return str(cls._BASE_RESOURCE_PATH / "styles" / style_name)