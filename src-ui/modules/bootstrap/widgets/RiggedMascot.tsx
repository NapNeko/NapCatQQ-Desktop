// 分层可动版吉祥物：同一张 cat_girl.svg，按 mascotRig 切成头 / 身 / 两只猫 / 地面五层。
// 只负责出 DOM；动起来靠 useMascotBody / useMascotFace 去抓 data-part 和 node-id。

import React, { useId, useMemo } from 'react';
import rawCatGirl from '../../../assets/cat_girl.svg?raw';
import { recolorMascot } from '../../../shared/components/next/Mascot';
import { cn } from '../../../shared/utils/cn';
import { buildRiggedMascotMarkup } from './mascotRig';

interface RiggedMascotProps {
    primaryColor: string;
    secondaryColor: string;
    className?: string;
}

export const RiggedMascot: React.FC<RiggedMascotProps> = ({ primaryColor, secondaryColor, className }) => {
    const prefix = `ndf-mascot${useId().replace(/[^a-zA-Z0-9]/g, '')}`;
    const markup = useMemo(
        () => buildRiggedMascotMarkup(recolorMascot(rawCatGirl, primaryColor, secondaryColor), prefix),
        [primaryColor, secondaryColor, prefix],
    );
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
