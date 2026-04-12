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


## 🔜 预告：Linux 版本即将到来

**v2.1 开发中** —— Desktop 即将跨平台！


## 🎉 v2.0 正式发布 - MSI 安装包来了！

**NapCatQQ-Desktop v2.0** 现已正式发布！这是一次重大更新，我们从单文件 EXE 全面升级为 MSI 安装包。

### MSI 安装包的好处

- **自动更新**：支持应用内一键更新，自动下载并安装新版本，无需手动下载
- **权限管理**：安装到 `C:\Program Files\NapCatQQ Desktop\`，运行更规范，安全性更高
- **配置隔离**：配置文件独立存储在 `%ProgramData%\NapCatQQ-Desktop\`，重装系统也不怕丢配置
- **干净卸载**：通过系统"应用和功能"即可完整卸载，不留残留
- **UAC 处理**：更新时自动申请管理员权限，无需右键"以管理员身份运行"

### 从旧版本迁移

**老用户升级指南**：

1. 点击设置 - 常规 - **"导入旧版配置"** 按钮，一键迁移之前的所有设置和 Bot 列表
2. 验证配置无误后即可正常使用

> 建议在迁移完成后备份一次配置

### 重要提示

v2.0 是重大版本更新，虽然经过了多轮测试，但仍可能存在一些我们未预料到的问题：

- 部分系统环境可能存在兼容性问题
- 极少数情况下自动更新可能需要手动干预
- 某些旧配置可能需要重新调整

**如果遇到任何问题，请不要犹豫，立即到 [GitHub Issues](https://github.com/NapNeko/NapCatQQ-Desktop/issues) 反馈**，我们会尽快修复！

感谢大家的理解与包容 🙏

## 关于项目

这个项目是为 [NapCatQQ](https://github.com/NapNeko/NapCatQQ) 提供管理界面（GUI），目的是让用户能够更快速、更直观的使用 NapCat

## 项目特点
- [x] **安装简单**: MSI 安装包，一键安装，干净卸载
- [x] **界面美观**: 使用 Fluent Design System 设计
- [x] **功能丰富**: 支持创建配置文件, 管理配置文件, 一键启动/停止/重启, 定时重启
- [x] **自动更新**: 支持应用内一键更新 NapCat Desktop，自动检测 NapCatQQ 更新

## 使用项目
可前往 [Releases](https://github.com/NapNeko/NapCatQQ-Desktop/releases) 下载最新版本的 **MSI 安装包**

> 系统要求：Windows 10 / Windows Server 2016 及以上版本


## 许可证

项目遵循 GPLv3 许可证，详情见[LICENSE](LICENSE)文件

## 如何卸载

如果你需要卸载本软件，可以通过以下方式：

**步骤一：系统设置卸载（推荐）**
- Windows 10/11：开始菜单 → 设置 → 应用 → 应用和功能 → 找到 "NapCatQQ Desktop" → 卸载
- 或运行 `appwiz.cpl` 打开程序和功能进行卸载

**步骤二：手动删除数据（可选）**
- 配置文件路径：`%ProgramData%\NapCatQQ-Desktop\`
- 卸载时数据文件默认保留，如需彻底清理请手动删除上述文件夹

> 💡 **小提示**：卸载前建议先导出配置备份，方便日后重新安装时恢复

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
