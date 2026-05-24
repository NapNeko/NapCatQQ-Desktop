// 主题化 mascot 组件。
//
// legacy `cat_girl.svg` 衣服两个原色（#6a95aa 主色 / #527388 深色）会被运行时
// 替换成当前主题的 brand 色，让吉祥物跟着主题色走（参考 legacy
// `cfg.themeColor` 触发 mascot 重绘的同款机制）。
//
// 实现思路：把 SVG 文本作为 ?raw 引入，简单 `String.replaceAll` 换两个固定 hex，
// 然后 dangerouslySetInnerHTML 渲染。SVG 内容是构建时定下来的可信资源，无 XSS 风险。
//
// 性能：每个 mount 做一次字符串替换（256KB SVG，主流机器 ms 级），useMemo 记忆。

import React, { useMemo } from 'react';
import rawCatGirl from '../../../assets/cat_girl.svg?raw';
import { cn } from '../../utils/cn';

interface MascotProps {
    /** 主色（衣服浅）。默认 brand-500。 */
    primaryColor?: string;
    /** 深色（衣服阴影 / 边缘）。默认 brand-700。 */
    secondaryColor?: string;
    className?: string;
    /** SVG 元素的 aria 属性，未传时 aria-hidden。 */
    label?: string;
}

// legacy 衣服 / 阴影固定色，确认过 SVG 文本里：
//   #6a95aa 出现 50 次（衣服主色）
//   #527388 出现 25 次（衣服阴影 + 深色边）
const LEGACY_PRIMARY = '#6a95aa';
const LEGACY_SECONDARY = '#527388';

function recolor(svgText: string, primary: string, secondary: string): string {
    return svgText
        .replaceAll(LEGACY_PRIMARY, primary)
        .replaceAll(LEGACY_PRIMARY.toUpperCase(), primary)
        .replaceAll(LEGACY_SECONDARY, secondary)
        .replaceAll(LEGACY_SECONDARY.toUpperCase(), secondary);
}

export const Mascot: React.FC<MascotProps> = ({
    primaryColor = '#ff8a4d',
    secondaryColor = '#c33f15',
    className,
    label,
}) => {
    const svgMarkup = useMemo(
        () => recolor(rawCatGirl, primaryColor, secondaryColor),
        [primaryColor, secondaryColor],
    );

    return (
        <div
            role={label ? 'img' : undefined}
            aria-label={label}
            aria-hidden={label ? undefined : true}
            className={cn('select-none', className)}
            // SVG 来自构建时打包的可信资源，无 XSS 风险
            dangerouslySetInnerHTML={{ __html: svgMarkup }}
        />
    );
};

export default Mascot;
