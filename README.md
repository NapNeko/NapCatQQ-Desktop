<div align="center">

<img src="src-ui/assets/logo.png" width="120" height="120" alt="NapCatQQ Desktop" />

# NapCatQQ Desktop

_Modern desktop console for NapCatQQ & SnowLuma._

装完就能管 Bot——本机起停、远端部署、看日志、装组件，一个窗口搞定。

[![V3](https://img.shields.io/badge/Desktop-V3%20WIP-E85D75?style=flat-square)](https://github.com/NapNeko/NapCatQQ-Desktop/tree/main)
[![License](https://img.shields.io/github/license/NapNeko/NapCatQQ-Desktop?style=flat-square)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2-FFC131?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-1.85+-DEA584?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/React-18-61DAFB?style=flat-square&logo=react&logoColor=black)](https://react.dev)
[![Stars](https://img.shields.io/github/stars/NapNeko/NapCatQQ-Desktop?style=flat-square)](https://github.com/NapNeko/NapCatQQ-Desktop/stargazers)

</div>

> **V3 还在准备中**  
> 代码在 [`main`](https://github.com/NapNeko/NapCatQQ-Desktop/tree/main) 推进，正式 MSI 还没发。Releases 里现成的桌面包仍是旧版，请先别当 V3 装。

---

## Welcome

- 给 [NapCatQQ](https://github.com/NapNeko/NapCatQQ) / [SnowLuma](https://github.com/SnowLuma/SnowLuma) 用的 **Windows 桌面端**。
- 现在是 **V3**：Rust + Tauri + React 重写，安装包更干净，杀软也少闹脾气。
- 旧 Python 树归档在 [`v1`](https://github.com/NapNeko/NapCatQQ-Desktop/tree/v1)，主线就是 `main`。

## Feature

- **上手快**
  - 装 MSI 就能用，配置和程序分开，重装不怕丢设置。
- **本机 + 远端**
  - 自己电脑能跑，Linux 也能管；SSH 连上就能部署。
- **双框架**
  - NapCat、SnowLuma 都能创建 / 启动 / 停止 / 看日志。
- **组件自己装**
  - Node、QQ、框架……页面里点安装就行，少抠命令行。
- **旧版可覆盖升**
  - 同 UpgradeCode，直接装新 MSI；数据在 `%ProgramData%\NapCatQQ Desktop`。

## Quick Start

### 用户

V3 正式包还没出。等发版后去 [Releases](https://github.com/NapNeko/NapCatQQ-Desktop/releases) 下：

```text
NapCatQQ-Desktop-<版本>-x64.msi
```

> Windows 10 / Server 2016+ · x64  
> 名字带 `watch-v` 的是远端监控小工具，不是桌面安装包。

从旧版过来（等 V3 MSI 上线后）：直接装新包覆盖 → 按提示迁配置 → 确认 Bot 列表还在 → 有空导出一份备份。

### 开发者（现在就能跑）

```bash
pnpm install
pnpm run tauri:dev
```

打安装包：

```bash
pnpm run tauri:build -- --bundles msi
```

常用检查：`pnpm run typecheck` · `pnpm run test:unit` · `pnpm run rust:test`

| 目录 | 干嘛的 |
|:--|:--|
| `crates/` | 后端业务 |
| `src-tauri/` | Tauri 壳和 MSI |
| `src-ui/` | 前端界面 |
| `docs/` | 文档 |

发版约定：`v*.*.*` → Desktop MSI · `watch-v*` → 远端监控二进制

## Link

| Download | [![Releases](https://img.shields.io/badge/GitHub-Releases-E85D75?style=flat-square)](https://github.com/NapNeko/NapCatQQ-Desktop/releases) | [![ncd-watch](https://img.shields.io/badge/ncd--watch-远端监控-7C9CBF?style=flat-square)](https://github.com/NapNeko/NapCatQQ-Desktop/releases?q=watch) |
|:-:|:-:|:-:|

| Community | [![QQ 群](https://img.shields.io/badge/QQ%20群-加入交流-12B7F5?style=flat-square&logo=tencentqq&logoColor=white)](https://qm.qq.com/q/8UK5ecfDyw) |
|:-:|:-:|

| Docs | [![用户说明](https://img.shields.io/badge/docs-用户说明-orange?style=flat-square)](docs/user/README.md) | [![代码地图](https://img.shields.io/badge/docs-代码地图-blue?style=flat-square)](docs/context/codemap.md) | [![NapCat Docs](https://img.shields.io/badge/docs%20on-Github.IO-orange?style=flat-square)](https://napneko.github.io/) |
|:-:|:-:|:-:|:-:|

| Framework | [![NapCatQQ](https://img.shields.io/badge/framework-NapCatQQ-6f42c1?style=flat-square)](https://github.com/NapNeko/NapCatQQ) | [![SnowLuma](https://img.shields.io/badge/framework-SnowLuma-00b4d8?style=flat-square)](https://github.com/SnowLuma/SnowLuma) |
|:-:|:-:|:-:|

## Thanks

- [NapCatQQ](https://github.com/NapNeko/NapCatQQ) · [SnowLuma](https://github.com/SnowLuma/SnowLuma)
- [Tauri](https://tauri.app) · [Rust](https://www.rust-lang.org) · [React](https://react.dev)
- 还有提 Issue、PR 和帮忙试的各位

<a href="https://github.com/NapNeko/NapCatQQ-Desktop/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=NapNeko/NapCatQQ-Desktop" alt="Contributors" />
</a>

## License

见 [LICENSE](LICENSE)。学习与合法用途使用，后果自负。
