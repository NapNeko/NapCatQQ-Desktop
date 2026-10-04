// 应用端框架在卡片上的头图。
//
// 图标是**前端资产**（放 src-ui/assets），不进 manifest：manifest 描述的是上游事实
// （名字、仓库、安装方式），而头图是桌面端的呈现选择，换一张不该动 Rust 与生成类型。
//
// 目前只有 NeoBot——它的头图取自带面板登录页的那张（面板 public/image/icon.webp），
// 桌面端这里存的是套过圆形遮罩的 128px PNG（原图是方形不透明的，白角贴深色卡片会露边）。
// 其余框架暂缺，缺了就不渲染，卡片退回纯标题。

import neobotLogo from '../../assets/neobot-logo.png';

const FRAMEWORK_LOGOS: Readonly<Record<string, string>> = {
    neobot: neobotLogo,
};

/** 框架头图；没有就返回 undefined（调用方不渲染占位） */
export function frameworkLogo(frameworkId: string): string | undefined {
    return FRAMEWORK_LOGOS[frameworkId];
}
