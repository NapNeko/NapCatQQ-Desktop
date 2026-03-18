![NapCatQQ-Desktop](https://socialify.git.ci/NapNeko/NapCatQQ-Desktop/image?font=Raleway&logo=https%3A%2F%2Fraw.githubusercontent.com%2FNapNeko%2FNapCatQQ%2Fmain%2Flogo.png&name=1&pattern=Circuit%20Board&stargazers=1&theme=Auto)

<div align="center">

[![License](https://img.shields.io/github/license/NapNeko/NapCatQQ-Desktop)](LICENSE)
[![Python](https://img.shields.io/badge/Python-3.12.*-green)](https://www.python.org/)
[![Code style: black](https://img.shields.io/badge/code%20style-black-000000.svg)](https://github.com/psf/black)
[![Imports: isort](https://img.shields.io/badge/%20imports-isort-%231674b1?style=flat&labelColor=ef8336)](https://pycqa.github.io/isort/)

![GitHub Repo stars](https://img.shields.io/github/stars/NapNeko/NapCatQQ-Desktop?style=social)
![GitHub forks](https://img.shields.io/github/forks/NapNeko/NapCatQQ-Desktop?style=social)
![GitHub watchers](https://img.shields.io/github/watchers/NapNeko/NapCatQQ-Desktop?style=social)

</div>

---

### **【恢复更新公告】NapCatQQ-Desktop 已恢复更新**

NapCatQQ-Desktop 已于 **2026 年 3 月恢复更新**。

过去 5 天内，仓库中实际发生提交的时间集中在 **2026 年 3 月 16 日到 2026 年 3 月 18 日**。这几天的工作主要集中在以下几个方向：

- **更新与打包链路恢复**：重做 Desktop ZIP 更新流程，补充备份、回滚、运行时目录打包控制，以及 PyInstaller 构建优化。
- **诊断能力补强**：补充 TRACE 日志、Qt 异常落盘、脱敏崩溃诊断包、开发者模式启动参数与开发者诊断入口。
- **核心稳定性修复**：修复配置加载与事务写入、Bot 配置编辑/删除、WebHook JSON 校验、下载器状态校验、主页跳转和引导安装流程等问题。
- **测试体系回补**：补充 `main`、`network`、`logger`、`run_napcat`、样式路径等针对性测试，并接入 `pytest-cov`。
- **界面与样式整理**：整理 UI 日志等级，统一样式资源入口，减少重复样式应用，并为共享样式资源增加回退策略。

### 当前状态

- 项目已不再处于“仅维护停更”状态，后续会继续恢复桌面端能力。
- **新版安装包正在整理中，我会很快带着新版包回来。**

### 接下来

- **WIP：组件库调整**
- 接下来一段时间会持续整理和调整项目内的组件库结构与样式资源，为后续版本迭代做准备。

如果您在当前版本中遇到问题，仍然欢迎通过项目的 **[GitHub Issues](https://github.com/NapNeko/NapCatQQ-Desktop/issues)** 页面反馈。

NapNeko组织 - AQiaoYo

2026年3月18日

## 关于项目

这个项目是为 [NapCatQQ](https://github.com/NapNeko/NapCatQQ) 提供管理界面（GUI），目的是让用户能够更快速、更直观的使用 NapCat

## 项目特点
- [x] **安装简单**: 单EXE文件，无需安装任何依赖
- [x] **界面美观**: 使用 Fluent Design System 设计
- [x] **功能丰富**: 支持创建配置文件, 管理配置文件, 一键启动/停止/重启, 定时重启
- [x] **自动更新**: 支持自动检查 NapCatQQ 更新, 一键更新

## 使用项目
可前往 [Releases](https://github.com/NapNeko/NapCatQQ-Desktop/releases) 下载最新版本的EXE文件

新版安装包正在整理中，完成后会尽快发布到 Releases。

如果您的系统是Windows Server 2016(或win10)及以上版本，可以使用本项目

## 回家归途
[QQ Group](https://qm.qq.com/q/X4CA6RnoIw)

## 许可证

项目遵循 GPLv3 许可证，详情见[LICENSE](LICENSE)文件

## 声明

- 此项目仅用于学习 PySide6，切勿用于违法犯罪行为。  
- 使用本项目所产生的一切后果均由使用者自行承担，与本项目作者及贡献者无关。  
- 本项目以“现状”提供，不提供任何明示或暗示的担保，包括但不限于适销性、特定用途适用性和非侵权性。  
- 在法律允许的最大范围内，作者和贡献者对于任何因使用本项目而产生的直接、间接、附带、特殊、惩罚性或后果性的损害概不负责，包括但不限于数据丢失、利润损失或业务中断。  
- 本声明同样适用于通过 GitHub Actions 或 releases 中的打包编译版本获取和使用本项目的用户。  
- 在使用本项目之前，请确保您已仔细阅读并完全理解此声明。如果您不同意此声明中的任何条款，请勿使用本项目。

## 鸣谢
- [NapCatQQ](https://github.com/NapNeko/NapCatQQ)
- [PySide6](https://wiki.qt.io/Qt_for_Python)
- [PyQt-Fluent-Widgets](https://github.com/zhiyiYo/PyQt-Fluent-Widgets)

## 贡献者 
> 🌟 星光闪烁，你们的智慧如同璀璨的夜空。感谢所有为 **NapCat Desktop** 做出贡献的人！

<a href="https://github.com/NapNeko/NapCatQQ-Desktop/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=NapNeko/NapCatQQ-Desktop" alt=""/>
</a>

![Alt](https://repobeats.axiom.co/api/embed/4078024c5df90cf42305ec425e68cfae76a1306d.svg "Repobeats analytics image")
