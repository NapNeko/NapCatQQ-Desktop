// 组件卡片标题左侧的头像。
//
// 头像是桌面端的呈现选择，不进 manifest / ComponentInfo：换图不该动 Rust 与生成类型。
// 图多取自上游 GitHub 组织头像（128px）；NeoBot 用的是它面板登录页那张，预先套了圆形遮罩。
// QQ 暂无合适的图，缺图的组件不渲染头像。

import type { ReactNode } from 'react';
import type { AppFrameworkId, ComponentId } from '../../core/ipc/types';
import desktopLogo from '../../assets/logo-72.png';
import napcatLogo from '../../assets/napcat-symbol-72.png';
import astrbotLogo from '../../assets/components/astrbot.png';
import gitLogo from '../../assets/components/git.png';
import karinLogo from '../../assets/components/karin.png';
import koishiLogo from '../../assets/components/koishi.png';
import maibotLogo from '../../assets/components/maibot.jpg';
import neobotLogo from '../../assets/components/neobot.png';
import nodejsLogo from '../../assets/components/nodejs.png';
import nonebot2Logo from '../../assets/components/nonebot2.png';
import novncLogo from '../../assets/components/novnc.png';
import redisLogo from '../../assets/components/redis.png';
import snowlumaLogo from '../../assets/components/snowluma.jpg';
import uvLogo from '../../assets/components/uv.png';
import vcredistLogo from '../../assets/components/vcredist.png';
import yunzaiLogo from '../../assets/components/yunzai.jpg';

// key 必须是 ComponentId 的 serde 字面量；应用端框架的 manifest.id 与之同源
const COMPONENT_LOGOS: Readonly<Partial<Record<ComponentId, string>>> = {
    napcat: napcatLogo,
    snowluma: snowlumaLogo,
    nodejs: nodejsLogo,
    novnc: novncLogo,
    vcredist: vcredistLogo,
    desktop_self: desktopLogo,
    ncd_watch: desktopLogo,
    uv: uvLogo,
    git: gitLogo,
    redis: redisLogo,
    karin: karinLogo,
    nonebot2: nonebot2Logo,
    astrbot: astrbotLogo,
    maibot: maibotLogo,
    koishi: koishiLogo,
    yunzai: yunzaiLogo,
    neobot: neobotLogo,
};

// 自家透明底图标，裁圆会切掉轮廓，按原样缩放
const UNCROPPED_LOGOS: ReadonlySet<string> = new Set([napcatLogo, desktopLogo]);

export function componentLogo(id: ComponentId | AppFrameworkId): string | undefined {
    return (COMPONENT_LOGOS as Readonly<Record<string, string | undefined>>)[id];
}

/** 卡片 icon 槽位用；缺图返回 undefined，卡片不留占位 */
export function componentLogoIcon(id: ComponentId | AppFrameworkId): ReactNode | undefined {
    const src = componentLogo(id);
    if (!src) return undefined;
    const fit = UNCROPPED_LOGOS.has(src) ? 'object-contain' : 'rounded-full object-cover';
    return (
        <img
            src={src}
            alt=""
            aria-hidden
            draggable={false}
            className={`h-6 w-6 select-none ${fit}`}
        />
    );
}
