# NapCatQQ Desktop 官网

官网源码独立维护于 `web` 分支。软件源码与用户指南以 `main` 为准。

## 本地预览

使用 Node.js 22.22+ 或 24，以及 pnpm。

```powershell
pnpm.cmd install
pnpm.cmd dev
```

`pnpm.cmd build` 构建静态页面到 `dist/`，`pnpm.cmd preview` 预览构建结果。

目前先实现首页。使用指南链接指向软件仓库的公开文档；后续文档站按明确的公开文件清单接入，不复制开发笔记或本地配置。

`SITE_BASE` 可设置构建时的站点前缀，以便后续部署到 GitHub Pages 项目路径。当前未配置线上部署。

## 素材

`public/media/` 的素材来源与演示数据说明见该目录的 README。
