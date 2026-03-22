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

### **【恢复更新公告】NapCatQQ-Desktop 持续更新中 🎉**

大家好！NapCatQQ-Desktop 从 **2026 年 3 月** 恢复更新以来，这周也没闲着，持续在搞事情～

**本周干了啥（3月16日 - 3月22日，狂肝 166 个提交）：**

这周主要围绕"让桌面端更好用"这个目标在努力：

- **UI 体验优化**：加了骨架屏让加载不那么枯燥，做了拖拽文件夹组件方便大家选路径，代码编辑器也支持日志高亮和主题切换了，看日志舒服多了
- **老用户福利**：做了遗留配置导入功能，之前用过的朋友升级后可以一键迁移旧配置，不用重新配一遍
- **问题定位更友好**：加了崩溃通知中心，程序出问题能更快知道原因；还做了脱敏诊断包导出，遇到问题方便抓 bug
- **更新更稳了**：桌面更新链路重构完成，支持 MSI 安装包更新了，更新过程应该更顺滑
- **代码整理**：把几个页面模块的结构重新梳理了一下，删了一堆冗余代码，现在清爽多了
- **测试补完**：接入了 pytest-cov，补了一批测试

**一些 bug 修复**：
配置保存、Bot 删除、WebHook 发送、下载器状态这些小问题都修了一波，应该用起来更顺了。

### 现在啥情况？

项目已经彻底活过来了，不是"只维护不更新"的状态了。**新版安装包正在打包测试，很快就能发出来！**

### 接下来打算搞啥

- 继续打磨 UI 组件，让界面更统一好看
- 日志查看体验再优化一下
- 版本检测逻辑再完善完善

总之就是在为后面的版本打基础，先把底子做扎实。

遇到啥问题随时来 **[GitHub Issues](https://github.com/NapNeko/NapCatQQ-Desktop/issues)** 找我，看到都会回的！

NapNeko组织 - AQiaoYo

2026年3月22日

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
