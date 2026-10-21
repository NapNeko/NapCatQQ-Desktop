# NapCatQQ Desktop 官网

官网源码独立维护于 `web` 分支。软件源码与用户指南以 `main` 为准。

`web` 使用独立历史，不合并回软件主线。此分支只保存网站源码、静态素材和构建配置，不包含桌面端源码或构建产物。

在软件工作区切换到此分支时，保留被忽略的本地文档、配置和参考资料，不要运行 `git clean -xfd`。本分支的忽略规则同时覆盖这些本地文件。

## 本地预览

使用 Node.js 22.22+ 或 24，以及 pnpm。

```powershell
pnpm.cmd install
pnpm.cmd dev
```

`pnpm.cmd build` 先构建教程中的前端演示，再构建静态页面到 `dist/`，`pnpm.cmd preview` 预览构建结果。

首页与 `/docs/` 文档站一起构建。文档包含从安装、创建 Bot、连接应用端到远端部署、聊天、调试和维护的教程，提供全文搜索、明暗主题、截图主题切换和大图查看。

`SITE_BASE` 可设置构建时的站点前缀，以便后续部署到 GitHub Pages 项目路径。当前未配置线上部署。

## 检查文档

```powershell
pnpm.cmd build
pnpm.cmd check:docs
```

`check:docs` 检查构建后的本地链接、锚点、图片、明暗配对和功能清单中的文档引用。CI 分别使用 `/` 和 `/NapCatQQ-Desktop/` 两种路径运行这些检查，不包含发布操作。

需要复测浏览器交互时，安装 Python 的 Playwright 包和 Chromium，先启动网站，再运行 `python scripts/smoke-docs.py --url http://127.0.0.1:4321`。可用 `--chromium` 指定浏览器路径，`--output` 指定报告目录；默认报告写到系统临时目录。它会检查搜索、主题、手机目录、截图、复制和页面资源。

在 PowerShell 中检查项目路径构建：

```powershell
$env:SITE_BASE = '/NapCatQQ-Desktop/'
pnpm.cmd build
pnpm.cmd check:docs
Remove-Item Env:SITE_BASE
```

## 维护教程

- 正文位于 `src/content/docs/*.md`。Frontmatter 的 `title`、`description`、`section`、`order` 决定标题、搜索描述、目录分组及阅读顺序。
- 正文链接写成 `/docs/first-bot/`；构建时自动补上 `SITE_BASE`。页内链接使用实际生成的标题锚点。
- 教程中的截图写成 `![说明](/docs/screenshots/名称-light.png)`，并同时放入同名的 `-dark.png`。页面会提供截图主题按钮和大图。素材记录与重拍流程见 `public/docs/screenshots/README.md`。
- 技术事实以软件 `main` 的公开用户指南和界面实现为准。本次核对基线为 `85cd7f40739d38a36c40d9d48ff5b390a2056cff`（Desktop 3.5.2）；版本提示在 `src/lib/docs.ts`，功能清单在 `scripts/docs-coverage.json`。
- 更新功能时同步操作步骤、成功状态、限制和失败处理，再复核功能清单。清单是人工审阅记录；关键词检查仅用于发现正文被删或引用失效，不能替代真实运行验证。
- 只读并整理明确公开的用户文档与相关实现，不导入 `docs/dev/`、配置、凭据或开发笔记。

构建依赖将 `@shikijs/vscode-textmate` 固定为已发布较久的 `10.0.2`，避免原锁文件中刚发布的传递依赖触发安装环境的最短发布时间限制。`package.json` 和 `pnpm-workspace.yaml` 保留相同覆盖项，分别兼容 pnpm 9 与新版本。

## 交互演示

`demo/` 使用固定软件版本的真实前端页面。源码从 Git 对象导出到忽略目录，不修改软件主线；首次构建需要安装该版本的前端依赖。准备与构建方法见 [demo/README.md](demo/README.md)。

## 素材

`public/media/` 的素材来源与演示数据说明见该目录的 README。
