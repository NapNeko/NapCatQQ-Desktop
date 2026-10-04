# 应用图标

`logo.png` 是 2026-10-02 定稿的 1024px RGBA 主图：珊瑚色猫耳终端、奶油色猫尾光标、低光泽材质。它是桌面和应用内品牌图的唯一栅格源，不要用平面 SVG 覆盖材质主图。

`logo-tray.svg` 是同一轮廓的小尺寸平面稿，减少留白并加粗笔画。托盘保留透明背景；运行状态带暖色点与浅色外圈，轻量模式降低饱和度。文件名里的 `light` 指轻量模式，与系统深浅主题无关。

安装项目依赖与 Pillow 后，在项目根执行：

```sh
pnpm icons:gen --dry-run
pnpm icons:gen
```

脚本调用项目锁定版本的 Tauri CLI 更新各平台图标，再生成 Windows 的七尺寸 ICO、窗口 PNG、应用内 32/48/72px 图与托盘 16/20/24/32/48px 三状态资源。接受新的定稿主图时可传 `--source /path/to/approved-1024.png`，先配合 `--dry-run` 检查范围。

侧栏、启动页、初始化面板、关于页和托盘面板共用 `logo*.png`。`cat_girl.svg` 是独立动画插画，`napcat.png` 是框架标识，不属于这套应用图标。

首页 NapCat 卡片使用原猫娘图形的 `napcat-symbol-{32,48,72}.png`，与 Desktop 主图分开。通过 `pnpm icons:gen --napcat-symbol-only --dry-run` 预览，再去掉 `--dry-run` 重建；来源为本仓库图标更新前的 Git 原图。

资源先在忽略入库的 `.cache/app-icons-*` 临时目录中生成，全部成功后才逐文件替换；内容一致的文件不重写，避免 Windows 构建器或预览器占用图标时原位覆盖失败。
