# 教程中的真实前端演示

版本记录在 `source.json`。当前基线是 Desktop 3.5.2 的 `85cd7f40739d38a36c40d9d48ff5b390a2056cff`。

构建脚本只从该 Git 对象导出 `src-ui/`、根 `package.json` 和 `pnpm-lock.yaml`，保存在 `.cache/desktop/`。不会切换或修改 `main`，不会复制 `crates/`、`src-tauri/`、本地文档、配置和被忽略文件。导出拒绝符号链接及 `src-ui/docs/`。

## 准备与构建

先查看导出清单和文件数，核对后再准备缓存：

```powershell
pnpm.cmd demo:plan
pnpm.cmd demo:prepare
```

`demo:plan` 不生成源码缓存或安装依赖。独立克隆的 `web` 分支可能没有基线 Git 对象，此时运行 `pnpm.cmd demo:plan -- --fetch-source`；它只额外获取固定对象，不检出、合并或更新任何分支。

`demo:prepare` 通过 `git archive` 导出源码，使用缓存中的锁文件独立执行 `pnpm install --frozen-lockfile --ignore-scripts --ignore-workspace`。不依赖本机存在软件工作区。需要 `git`、`tar`、Node.js 22.22+ 和 pnpm。

```powershell
pnpm.cmd dev
```

`dev` 启动前会先构建演示。编译产物位于 `public/demo/`，可打开 `/demo/`；单独重建用 `pnpm.cmd build:demo`。产物使用相对资源路径 `./`，网站位于 GitHub 项目路径时也可加载。`pnpm.cmd build` 会先生成演示，再构建整个网站；缓存与演示产物均已忽略，不入库。

本机调试可显式复用软件已安装的依赖：

```powershell
$env:DESKTOP_DEMO_NODE_MODULES = 'D:\NapCat-Project\NapCatQQ-Desktop-V1\node_modules'
pnpm.cmd demo:prepare
pnpm.cmd build:demo
```

复用通过 Vite 解析配置直接读取目标依赖，不创建目录链接，不安装或修改目标目录。取消该变量后默认使用缓存中的独立安装。缓存基线变化时先移走 `.cache/desktop/`，重新查看导出计划，不清理软件工作区。

`pnpm.cmd test:demo` 检查配置保存、启停取消和重播状态；CI 还会执行 `demo/tsconfig.json` 的类型检查，以及两种网站 base 路径的构建检查。

本地检查项目路径时可用 `astro build --outDir .cache/site-base-check`，并设置 `SITE_BASE=/NapCatQQ-Desktop/`、`DOCS_CHECK_OUT_DIR=.cache/site-base-check` 后运行文档检查，避免覆盖正在预览的 `dist/`。

## 教程场景

`lessons/` 定义每篇的初始状态、真实控件操作和手动进度；`player.ts` 共用播放、暂停、接管、重置与主题同步。`src/lib/tutorials.ts` 记录文档入口、预览图与步骤标签。新增场景时，三处 ID 和步骤数量需要一致。

可用场景为 `first-bot`、`components`、`connections`、`bots`、`tasks-logs`、`settings`、`network`、`backup`、`overview`、`apps`、`debugging`、`chat`。独立预览例如 `/demo/?lesson=connections`。所有教程都由 `TutorialDemo.astro` 渲染同一套窗口，不复制播放器。

`system.mock.ts` 和 `workspace.mock.ts` 只在浏览器入口替换服务，分别隔离组件/设置/缓存和应用/调试/聊天样例。重置先清任务与日志，再重新准备当篇数据。原始前端通过动态 import 按页面加载，初次打开文档不会加载整个软件。

文档中的重置和重新演示会重新加载 iframe，清理复用前端的模块缓存与计时器；暂停、继续和放大保留当前页面。

## 构建边界

- `@desktop` 与 `@` 指向缓存中的原始 `src-ui`；`@demo` 指向本目录。
- 仅将原始 `core/ipc/mock/bot.mock.ts`、`events.mock.ts` 的导入替换为本目录同名教学适配。页面、表单和事件 hook 来自固定软件版本。
- 两处构建适配会断言原文恰好匹配一次：`BotAvatar.tsx` 使用原有 initials 回退，避免向 QQ 头像服务发送演示账号；`preferencesStore.ts` 改用 `ncd:docs-demo:preferences:v1`，隔离教学偏好。它们只改变编译输入，不改缓存源码或 `main` 文件。
- `demo.css` 导入原始全局 CSS，并显式声明 Tailwind 扫描目录，避免 `.cache` 的忽略规则漏掉真实 UI 样式。
- 教学场景模拟进程与日志，不连接 QQ、不执行安装或 SSH 操作。真实扫码、设备验证和外部 WebUI 仍按教程说明完成。
