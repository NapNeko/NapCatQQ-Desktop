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

`pnpm.cmd build` 先构建教程中的前端演示，再生成图片派生文件并构建静态页面到 `dist/`，`pnpm.cmd preview` 预览构建结果。

首页与 `/docs/` 文档站一起构建。文档包含从安装、创建 Bot、连接应用端到远端部署、聊天、调试和维护的教程，提供全文搜索、明暗主题、浮动阅读导航和截图大图查看。截图与演示界面跟随文档主题。

首页使用本地打包的 GSAP / ScrollTrigger：首屏猫形终端分层展开、软件窗口随滚动推近，随后横向浏览上线、接入应用端与聊天三个场景。功能区使用标题遮罩、截图景深与路径绘制衔接。动画入口为 `src/scripts/home-motion.ts`，视觉样式在 `src/styles/home.css`、`hero.css` 与 `story.css`，叙事内容在 `src/components/WorkflowStory.astro`。宽度至少 1050px、高度至少 740px 且使用精细指针时启用固定场景；触屏、窄屏和矮屏使用纵向布局与较小的滚动位移。系统开启「减少动态效果」或关闭 JavaScript 时，页面保持完整静态排版。键盘聚焦产品或教程入口会同步将对应画面带入视口。

`SITE_BASE` 可设置构建时的站点前缀；GitHub Pages 使用 `/NapCatQQ-Desktop/`。

首页各功能区复用 `TutorialDemo.astro` 和文档中的真实前端。程序页面和教程脚本按当前场景加载，聊天、终端等模块仅在使用时下载。自动演示共享两个常驻名额，离屏先暂停，12 秒后回收；手动操作、主动暂停和展开中的演示保留状态。宽度不超过 760px 时先显示封面，点击后再加载。用户主动打开的演示优先于自动加载预算。

「自己操作」在当前窗口接管，点击「放大」才进入 top layer；收起保留操作状态。放大画布同时适配可用宽高，窄屏在窗口内滚动。演示和截图跟随页面主题，切换主题不重载演示。

首次进入会播放约 3.3 秒的开场：字标展开、猫标分层组装、窗口展开，再在同一浅色背景上衔接首屏。标记保存在会话存储中，同一会话返回或刷新不会重复播放；首屏底部可以重播。跳过、滚动、窗口尺寸变化和减少动态效果都会结束开场。动画不等待演示资源，加载失败也不阻塞阅读。开场入口为 `src/scripts/home-opening.ts`，首屏用 CSS sticky 配合 ScrollTrigger，避免 pin 重挂 iframe 引发演示重载。下半页的主机拓扑、应用端叠层、聊天气泡和值守波形由 `FeatureArt.astro`、`home-features.css` 与同一滚动动画入口维护。

`/changelog/` 在站内展示 Desktop 和 ncd-watch 的发版说明，支持版本索引、历史展开、预览版本筛选和版本链接复制。主题选择与文档共用；首页的版本入口直接定位到对应日志。

发版内容来自公开 GitHub Releases，快照保存在 `src/data/releases.json`。运行 `pnpm.cmd releases:plan` 预览同步范围，再用 `pnpm.cmd releases:sync` 更新快照；本地已登录 GitHub CLI 时可追加 `--gh`。网站构建读取快照，不要求联网，不会修改软件分支。正文保留原始发版内容，下载与完整提交等附录单独折叠；日期来自发布时间，下载直达对应版本资源。

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

`pnpm.cmd report:assets` 根据构建清单统计 JS/CSS 的原始字节与 gzip 字节，并合并入口、教程和页面的依赖，列出各场景首次可操作时所需的资源。gzip 数字用于比较产物体积，不代表浏览器实测传输、CPU 或内存占用。

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

`pnpm.cmd images:plan` 查看图片处理计划，`pnpm.cmd images:prepare` 生成带内容 hash 的 WebP 和清单到被忽略的 `public/optimized/`。原始截图和来源记录保留；完整截图采用无损压缩，较小版本仅在体积更小时用于响应式加载。Logo 按展示尺寸生成。构建和开发服务会自动准备这些文件，手动修改源图后需重新生成。明暗截图共用一个图片节点，避免同时下载两套。
