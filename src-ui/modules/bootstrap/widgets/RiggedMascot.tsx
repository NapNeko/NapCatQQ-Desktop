// 分层可动版吉祥物：同一张 cat_girl.svg，按 mascotRig 切成头 / 身 / 两只猫 / 地面五层。
// 只负责出 DOM；动起来靠 useMascotBody / useMascotFace 去抓 data-part 和 node-id。

import React from 'react';
import rawCatGirl from '../../../assets/cat_girl.svg?raw';
import { recolorMascot } from '../../../shared/components/next/Mascot';
import { cn } from '../../../shared/utils/cn';
import { buildRiggedMascotMarkup } from './mascotRig';

interface RiggedMascotProps {
    primaryColor: string;
    secondaryColor: string;
    className?: string;
}

// 素材 256 KB，切五层要把每条 path 的 d 都扫一遍算包围盒，一次几十毫秒。
// useMemo 挡不住路由来回切——组件一卸载记忆就没了，回首页又得重算。
// 同一套颜色出来的结果完全一样，缓存在模块上。
// clipPath 的 id 因此是固定的：同屏只会有一只吉祥物，真同时挂两只也是指向同样的定义。
const markupCache = new Map<string, string>();

function riggedMarkup(primaryColor: string, secondaryColor: string): string {
    const key = `${primaryColor}|${secondaryColor}`;
    let markup = markupCache.get(key);
    if (markup === undefined) {
        markup = buildRiggedMascotMarkup(recolorMascot(rawCatGirl, primaryColor, secondaryColor), 'ndf-mascot-rig');
        markupCache.set(key, markup);
    }
    return markup;
}

export const RiggedMascot: React.FC<RiggedMascotProps> = ({ primaryColor, secondaryColor, className }) => {
    const markup = riggedMarkup(primaryColor, secondaryColor);
    return (
        <div
            aria-hidden
            className={cn('select-none', className)}
            // 构建期打包的可信资源，无 XSS 风险
            dangerouslySetInnerHTML={{ __html: markup }}
        />
    );
};

export default RiggedMascot;
