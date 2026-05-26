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

<img width="1722" height="1080" alt="6d3def2669ba6e01bc278c4df8c50761" src="https://github.com/user-attachments/assets/cd5dd4de-1e02-4970-bb19-9ac79b2aa142" />


## 关于项目

这个项目是为 [NapCatQQ](https://github.com/NapNeko/NapCatQQ) 提供管理界面（GUI），目的是让用户能够更快速、更直观的使用 NapCat。

## 项目特点

- [x] **远程管理**：通过 SSH 部署、运行、监控 Linux 服务器上的 NapCat（v2.1 新增）
- [x] **本地管理**：创建/管理配置文件，一键启动/停止/重启，定时重启，日志查看
- [x] **MSI 安装包**：一键安装、自动更新、配置与程序分离（配置独立存于 `%ProgramData%\NapCatQQ-Desktop\`）
- [x] **界面美观**：基于 Fluent Design System，深浅主题与多语言支持
- [x] **资源监控**：实时查看本地与远端的 CPU、内存、Bot 进程状态
- [x] **安全可靠**：主机密钥校验、keyring 凭据存储、友好错误提示
- [x] **自动更新**：应用内一键更新 NapCat Desktop，自动检测 NapCatQQ 更新

## v2.x 预告：
谁还古法编程开发插件？

## v2.2 新功能亮点：SnowLuma支持

### 新增功能
- 全新引入 SnowLuma 框架，支持远程后端管理与 daemon 驱动层
- Bot 页面新增 SnowLuma 启动与运行状态展示
- 添加远程部署脚本资源，简化远程部署流程
- 实现 SnowLuma 远程管理核心功能，包括 SSH 连接、会话管理、VNC 启动等
- 添加 SnowLuma 远程后端与运行时支持，实现本地回退机制
- 新增 SnowLuma 远程管理 UI 组件，提供可视化操作界面
- 完善 SnowLuma 后端支持，覆盖配置渲染、状态轮询、WebUI 客户端等

## 使用项目

可前往 [Releases](https://github.com/NapNeko/NapCatQQ-Desktop/releases) 下载最新版本的 **MSI 安装包**。

> 系统要求：Windows 10 / Windows Server 2016 及以上版本

## 从旧版本升级

v1 老用户迁移到 v2 后：

1. 安装 v2 MSI，打开设置 → 常规 → **"导入旧版配置"**，一键迁移之前的设置和 Bot 列表
2. 验证配置无误后备份一次，即可正常使用

遇到问题请到 [GitHub Issues](https://github.com/NapNeko/NapCatQQ-Desktop/issues) 反馈，我们会尽快处理。

## 许可证

项目遵循 GPLv3 许可证，详情见 [LICENSE](LICENSE) 文件。

## 如何卸载

**步骤一：卸载程序（推荐）**

- Windows 10/11：开始菜单 → 设置 → 应用 → 应用和功能 → 找到 "NapCatQQ Desktop" → 卸载
- 或运行 `appwiz.cpl` 打开程序和功能进行卸载

**步骤二：清理用户数据（可选）**

卸载只会移除程序本体，**用户配置默认保留在 `%ProgramData%\NapCatQQ-Desktop\`**，方便重装时恢复。如果你不再需要这些数据，可手动删除该目录彻底清理。

> 彻底清理前建议先在设置中导出一次配置备份，方便日后恢复。

## 声明

- 此项目仅用于学习 PySide6，切勿用于违法犯罪行为。  
- 使用本项目所产生的一切后果均由使用者自行承担，与本项目作者及贡献者无关。  
- 本项目以“现状”提供，不提供任何明示或暗示的担保，包括但不限于适销性、特定用途适用性和非侵权性。  
- 在法律允许的最大范围内，作者和贡献者对于任何因使用本项目而产生的直接、间接、附带、特殊、惩罚性或后果性的损害概不负责，包括但不限于数据丢失、利润损失或业务中断。  
- 本声明同样适用于通过 GitHub Actions 或 releases 中的打包编译版本获取和使用本项目的用户。  
- 在使用本项目之前，请确保您已仔细阅读并完全理解此声明。如果您不同意此声明中的任何条款，请勿使用本项目。

## 鸣谢
- [futureppo(F一串)的公益站大力支持！](https://blog.futureppo.top)
- [NapCatQQ](https://github.com/NapNeko/NapCatQQ)
- [PySide6](https://wiki.qt.io/Qt_for_Python)
- [PyQt-Fluent-Widgets](https://github.com/zhiyiYo/PyQt-Fluent-Widgets)

## 贡献者 
> 感谢所有为 **NapCat Desktop** 做出贡献的人。

<a href="https://github.com/NapNeko/NapCatQQ-Desktop/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=NapNeko/NapCatQQ-Desktop" alt=""/>
</a>

![Alt](https://repobeats.axiom.co/api/embed/4078024c5df90cf42305ec425e68cfae76a1306d.svg "Repobeats analytics image")
