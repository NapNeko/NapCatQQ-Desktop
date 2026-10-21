# 教程截图

截图使用软件 `main` 的真实前端和仓库自带的浏览器 IPC 演示数据，来源提交为 `85cd7f40739d38a36c40d9d48ff5b390a2056cff`（Desktop 3.5.2）。账号、消息、模型、终端输出和主机信息均用于演示；它们不能作为真实 QQ 登录、SSH 部署、命令执行或模型请求成功的记录。

每个操作状态保留 `名称-light.png` 与 `名称-dark.png`，尺寸统一为 **1440 × 1000**、设备像素比 1。同一页面使用客户端支持的「跟随系统」主题，通过浏览器 `prefers-color-scheme` 切换浅色和深色。截图取自运行界面，未重绘产品窗口。

## 重拍

先检出对应版本的软件 `main`，安装锁文件中的依赖。浏览器直接打开前端时会使用项目自带的预览数据。开发预览可以使用 `pnpm dev`；批量截图建议构建静态前端后运行本地 HTTP 服务，减少动态依赖预构建的影响。

本次采集的 Linux 工作区使用以下方式启动静态预览（在软件工作区执行）：

```bash
pnpm exec vite build --config src-ui/vite.config.ts --outDir /tmp/ncd-docs-ui
python -m http.server 1422 --bind 127.0.0.1 --directory /tmp/ncd-docs-ui
```

在网站工作区安装 Python Playwright 并准备 Chromium，再运行对应采集脚本。脚本的 `--url`、`--chromium`、`--output` 可覆盖默认地址、浏览器和输出路径；`--main-sha` 填正在运行的完整软件提交号。

```bash
python scripts/capture-core-screenshots.py --main-sha 85cd7f40739d38a36c40d9d48ff5b390a2056cff
python scripts/capture-bot-screenshots.py --main-sha 85cd7f40739d38a36c40d9d48ff5b390a2056cff
python scripts/capture-app-screenshots.py --main-sha 85cd7f40739d38a36c40d9d48ff5b390a2056cff
python scripts/capture-framework-screenshots.py --main-sha 85cd7f40739d38a36c40d9d48ff5b390a2056cff
python scripts/capture-chat-terminal-screenshots.py --main-sha 85cd7f40739d38a36c40d9d48ff5b390a2056cff
```

各 `*-manifest.json` 记录采集页面和来源，包含哈希的清单用于核对具体素材。远端添加表单的过程与能力限制见 `REMOTE-CAPTURES.md`。重拍时同步清单，再执行网站构建和 `pnpm check:docs`，确认两种主题的图片与正文引用一致。

## 采集范围

- 概览、组件、机器人、新增 Bot、连接编辑、日志、应用端、七种框架面板、聊天、群文件、调试台、终端、任务和设置。
- 新增 Bot 截图使用未保存的示例 QQ 号；远端添加表单使用文档保留地址，不填真实密码。
- Windows 原生安装器、截图选区、系统托盘和实际登录二维码需要 Windows 实机补拍。浏览器预览也不能保存真实 SSH 主机或进入依赖远端 Docker 的容器管理页。这些功能保留源码核对后的操作教程，未配不相符的界面图。

更新时请关闭与当前步骤无关的启动提示，检查图片、输入框和页面状态。只保留演示数据，避免上传账号凭据、私人消息或服务器信息。
