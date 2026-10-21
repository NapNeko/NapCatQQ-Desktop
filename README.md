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

首页与 `/docs/` 文档站一起构建。文档包含从安装、创建 Bot、连接应用端到远端部署、聊天、调试和维护的教程，提供全文搜索、明暗主题、浮动阅读导航和截图大图查看。截图与演示界面跟随文档主题。

首页使用本地打包的 GSAP / ScrollTrigger：首屏标题揭幕、产品截图视差、功能区滚动入场，以及桌面宽屏的三段横向叙事。动画入口为 `src/scripts/home-motion.ts`，视觉样式在 `src/styles/home.css` 与 `hero.css`，叙事内容在 `src/components/WorkflowStory.astro`。触屏、窄屏和矮屏采用自然排版；系统开启「减少动态效果」时保留静态内容，关闭 JavaScript 也能阅读功能介绍并进入文档或下载。

`SITE_BASE` 可设置构建时的站点前缀；GitHub Pages 使用 `/NapCatQQ-Desktop/`。

## 线上发布

站点地址为 <https://napneko.github.io/NapCatQQ-Desktop/>，教程位于站点下的 `docs/`。

推送到 `web` 后，`.github/workflows/docs.yml` 先完成两种路径的构建、文档检查、演示类型检查和测试，再将项目路径构建的 `dist/` 上传并发布到 GitHub Pages。Pull Request 只检查，不发布。发布失败时，到 Actions 查看对应步骤，修复后重新推送或重跑失败任务。

仓库的 Pages 发布来源应保持为 GitHub Actions，`github-pages` 环境需允许 `web` 分支。源码和发布流程均留在 `web`，无需将网站合并到 `main` 或另建产物分支。

## 检查文档

```powershell
pnpm.cmd build
pnpm.cmd check:docs
```

`check:docs` 检查构建后的本地链接、锚点、图片、明暗配对和功能清单中的文档引用。CI 分别使用 `/` 和 `/NapCatQQ-Desktop/` 两种路径运行这些检查；仅 `web` 的推送在全部检查通过后发布。

需要复测浏览器交互时，安装 Python 的 Playwright 包和 Chromium，先启动网站，再运行 `python scripts/smoke-docs.py --url http://127.0.0.1:4321`。可用 `--chromium` 指定浏览器路径，`--output` 指定报告目录；默认报告写到系统临时目录。它会检查所有教程在宽窄屏、明暗主题下的表格与演示工具栏，并检查浮动导航、播放控制、搜索、截图、复制和页面资源。

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
- 教程中的截图写成 `![说明](/docs/screenshots/名称-light.png)`，并同时放入同名的 `-dark.png`。页面根据文档主题显示对应截图，点击可看大图。素材记录与重拍流程见 `public/docs/screenshots/README.md`。
- 技术事实以软件 `main` 的公开用户指南和界面实现为准。本次核对基线为 `85cd7f40739d38a36c40d9d48ff5b390a2056cff`（Desktop 3.5.2）；版本提示在 `src/lib/docs.ts`，功能清单在 `scripts/docs-coverage.json`。
- 更新功能时同步操作步骤、成功状态、限制和失败处理，再复核功能清单。清单是人工审阅记录；关键词检查仅用于发现正文被删或引用失效，不能替代真实运行验证。
- 只读并整理明确公开的用户文档与相关实现，不导入 `docs/dev/`、配置、凭据或开发笔记。

构建依赖将 `@shikijs/vscode-textmate` 固定为已发布较久的 `10.0.2`，避免原锁文件中刚发布的传递依赖触发安装环境的最短发布时间限制。`package.json` 和 `pnpm-workspace.yaml` 保留相同覆盖项，分别兼容 pnpm 9 与新版本。

## 交互演示

`demo/` 使用固定软件版本的真实前端页面。源码从 Git 对象导出到忽略目录，不修改软件主线；首次构建需要安装该版本的前端依赖。准备与构建方法见 [demo/README.md](demo/README.md)。

## 素材

`public/media/` 的素材来源与演示数据说明见该目录的 README。
