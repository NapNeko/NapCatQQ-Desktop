# NapCatQQ Desktop 文档

[项目首页](../README.md) · [下载安装包](https://github.com/NapNeko/NapCatQQ-Desktop/releases/latest) · [更新日志](https://github.com/NapNeko/NapCatQQ-Desktop/releases)

## 使用 Desktop

| 目标                                        | 文档                                             |
| :------------------------------------------ | :----------------------------------------------- |
| 首次安装、创建 Bot、备份与常见问题          | [用户指南](user/README.md)                       |
| 安装应用端、导入项目与对接 Bot              | [应用端实例](user/app-instances.md)              |
| 聊天、媒体、群文件、截图与账号托盘          | [聊天与截图](user/chat.md)                       |
| 添加 SSH 主机、原生 / Docker 部署与远端排错 | [远端 Linux 管理](user/remote-linux.md)          |
| Desktop 退出后的监控与通知                  | [ncd-watch 远端值守](user/ncd-watch.md)          |
| SnowLuma 扫码登录                           | [简化扫码登录](user/snowluma-remote-qr-login.md) |

## 参与开发

| 目标                            | 文档                                    |
| :------------------------------ | :-------------------------------------- |
| 配置开发环境、运行检查、提交 PR | [贡献指南](../CONTRIBUTING.md)          |
| 按功能定位源码                  | [代码地图](context/codemap.md)          |
| 查找已有后端能力                | [后端能力速查](context/capabilities.md) |
| 了解前端分层与组件约定          | [前端约定](context/frontend.md)         |
| 查阅历史问题与约束              | [开发经验](context/lessons.md)          |
| 编写和预览发布说明              | [发版说明维护](releases/README.md)      |

完整版本更新以 [GitHub Releases](https://github.com/NapNeko/NapCatQQ-Desktop/releases) 为准。`docs/releases/` 保存各版策展正文，`v*.*.*` 对应 Desktop，`watch-v*` 对应远端监控组件。

## 文档维护

| 目录        | 内容                               | 版本控制 |
| :---------- | :--------------------------------- | :------- |
| `user/`     | 面向用户的操作步骤、限制与排错     | 入库     |
| `context/`  | 可复用的代码地图、架构与开发约定   | 入库     |
| `releases/` | 各版本发布说明及维护流程           | 入库     |
| `assets/`   | 文档使用的截图等资源               | 入库     |
| `dev/`      | 本地计划、排查记录、临时报告与归档 | 默认忽略 |

- README 负责介绍产品与入口，操作细节写进用户文档，通过链接引用。
- 功能变化时同步对应指南；注明版本、权限和运行条件，不把计划中的能力写成已发布功能。
- 截图优先使用演示数据并说明来源，不上传账号凭据、真实消息或私人服务器信息。
- 本地工作记录留在已忽略的 `docs/dev/`；`work/`、`docs/superpowers/` 也只作本地记录。`.gitignore` 不会使已跟踪文件自动退出版本控制。
- 本地笔记也不保存凭据或访问令牌，忽略规则不限制文件访问，也不能阻止其他方式上传。
- 取消跟踪已入库的本地记录前，确认用途并检查引用，再用 `git rm --cached -- <具体路径>` 保留本地副本。
- 正式文档按具体路径暂存；提交前检查改动和链接，不强制添加被忽略的本地记录。
