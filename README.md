![NapCatQQ-Desktop](https://socialify.git.ci/HeartfeltJoy/NapCatQQ-Desktop/image?font=Raleway&logo=https%3A%2F%2Fraw.githubusercontent.com%2FNapNeko%2FNapCatQQ%2Fmain%2Flogo.png&name=1&pattern=Circuit%20Board&theme=Auto)


## 关于项目

这个项目是为 [NapCatQQ](https://github.com/NapNeko/NapCatQQ) 提供的图形用户界面（GUI），目的是让用户能够更快速、更直观地创建配置文件和启动脚本

## 开发进度

### 已实现
 - 基本窗体
 - 基本 Ui
 - 创建 Ps1 启动脚本

### 待实现
 - 创建 bat、sh 启动脚本
 - 实现管理机器人实力
 - 在程序内实现启动 NapCatQQ 并监控

## 开始上手

### 前置需求
目前还在开发阶段,核心功能并未完成,如想运行请确保你包括以下环境:

- Python 3.9以上版本
- 想了一下好像没了

### 安装
1. 克隆仓库到你本地

```bash
git clone https://github.com/ruixiaotian/NapCat-Desktop.git
```

2. 进入项目目录
```bash
cd NapCat-Desktop
```

3. 创建虚拟环境(可选,此处使用venv)

```bash
python -m venv venv
```

4. 安装依赖

```bash
pip install -r requirements.txt
```

5. 编译资源文件(此处需要找到你pyside6-rcc.exe路径)
```bush
pyside6-rcc.exe src/ui/resource/resource.qrc -o src/ui/resource/resource.py
```

### 启动

```bash
python src/main.py
```

## 许可证

这个项目遵循 GPLv3 许可证，详情见[LICENSE](LICENSE)文件

## 声明

- 此项目仅用于学习 PySide6

## 鸣谢
- [NapCatQQ](https://github.com/NapNeko/NapCatQQ)
- [PySide6](https://wiki.qt.io/Qt_for_Python)
- [PyQt-Fluent-Widgets](https://github.com/zhiyiYo/PyQt-Fluent-Widgets)
- [GraiaProject - creart](https://github.com/GraiaProject/creart)
