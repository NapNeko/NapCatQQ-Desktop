![NapCatQQ Desktop](https://socialify.git.ci/NapNeko/NapCatQQ-Desktop/image?description=1&descriptionEditable=Modern%20desktop%20console%20for%20NapCatQQ%20%26%20SnowLuma&font=Inter&forks=1&issues=1&logo=https%3A%2F%2Fraw.githubusercontent.com%2FNapNeko%2FNapCatQQ-Desktop%2Fmain%2Fsrc-ui%2Fassets%2Flogo.png&name=1&owner=1&pattern=Diagonal%20Stripes&stargazers=1&theme=Dark)

<div align="center">

[![License](https://img.shields.io/github/license/NapNeko/NapCatQQ-Desktop?style=flat-square)](LICENSE)
[![Release](https://img.shields.io/github/v/release/NapNeko/NapCatQQ-Desktop?style=flat-square&color=E85D75)](https://github.com/NapNeko/NapCatQQ-Desktop/releases)
[![Tauri](https://img.shields.io/badge/Tauri-2-FFC131?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-1.85+-DEA584?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Stars](https://img.shields.io/github/stars/NapNeko/NapCatQQ-Desktop?style=flat-square)](https://github.com/NapNeko/NapCatQQ-Desktop/stargazers)

**[下载安装包](https://github.com/NapNeko/NapCatQQ-Desktop/releases)** · **[加入 QQ 群](https://qm.qq.com/q/8UK5ecfDyw)** · **[用户文档](docs/user/README.md)**

</div>

---

## 关于

**NapCatQQ Desktop** 是 [NapCatQQ](https://github.com/NapNeko/NapCatQQ) / [SnowLuma](https://github.com/SnowLuma/SnowLuma) 的 Windows 桌面控制台。

装完就能管 Bot：本机起停、远端部署、看日志、装组件，一个窗口搞定。

当前主线是 **V3**（Rust + Tauri + React）。旧 Python 实现归档在 [`v1`](https://github.com/NapNeko/NapCatQQ-Desktop/tree/v1)。

## 特性

- **本机 + 远端** — 自己电脑能跑，Linux 也能管；SSH 连上就能部署
- **双框架** — NapCat、SnowLuma 都能创建 / 启动 / 停止 / 看日志
- **组件自助装** — Node、QQ、框架……页面里点安装，少抠命令行
- **配置与程序分离** — 数据在 `%ProgramData%\NapCatQQ Desktop`，重装不怕丢设置
- **旧版可覆盖升** — 同 UpgradeCode，直接装新 MSI

## 安装

1. 打开 [Releases](https://github.com/NapNeko/NapCatQQ-Desktop/releases)
2. 下载 `NapCatQQ-Desktop-<版本>-x64.msi`
3. 双击安装，打开就能用

> 系统要求：Windows 10 / Server 2016+ · x64  
> 名字带 `watch-v` 的是远端监控小工具，不是桌面安装包。

### 从旧版升级

直接装新 MSI 覆盖 → 按提示迁配置 → 确认 Bot 列表还在 → 有空导出一份备份。

## 开发

```bash
pnpm install
pnpm run tauri:dev
```

```bash
# 打 MSI
pnpm run tauri:build -- --bundles msi

# 常用检查
pnpm run typecheck
pnpm run test:unit
pnpm run rust:test
```

| 目录 | 用途 |
| --- | --- |
| `crates/` | 后端业务 |
| `src-tauri/` | Tauri 壳与 MSI |
| `src-ui/` | 前端界面 |
| `docs/` | 文档 |

发版约定：`v*.*.*` → Desktop MSI · `watch-v*` → 远端监控二进制

## 社区

- QQ 群：[点击加入](https://qm.qq.com/q/8UK5ecfDyw)
- 问题反馈：[GitHub Issues](https://github.com/NapNeko/NapCatQQ-Desktop/issues)

## 相关链接

- [用户说明](docs/user/README.md)
- [代码地图](docs/context/codemap.md)
- [NapCat 文档](https://napneko.github.io/)
- [NapCatQQ](https://github.com/NapNeko/NapCatQQ)
- [SnowLuma](https://github.com/SnowLuma/SnowLuma)
- [ncd-watch 发布](https://github.com/NapNeko/NapCatQQ-Desktop/releases?q=watch)

## 致谢

- [NapCatQQ](https://github.com/NapNeko/NapCatQQ) · [SnowLuma](https://github.com/SnowLuma/SnowLuma)
- [Tauri](https://tauri.app) · [Rust](https://www.rust-lang.org) · [React](https://react.dev)
- 还有提 Issue、PR 和帮忙试的各位

<div align="center">
  <a href="https://github.com/NapNeko/NapCatQQ-Desktop/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=NapNeko/NapCatQQ-Desktop" alt="Contributors" />
  </a>
</div>

## 许可

见 [LICENSE](LICENSE)。学习与合法用途使用，后果自负。
