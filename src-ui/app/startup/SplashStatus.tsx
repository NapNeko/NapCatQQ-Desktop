// 启动屏文字与进度区：主标题 + 副标题（含呼吸点）、激光进度条、版本号。
// 从 StartupSplash 主件原样搬移的纯展示 JSX；作为 fragment 直挂在 stage 容器下，
// 与吉祥物并列，保持原有的层级与 gap 节奏不变。副标题文字由换词动画经 subTextRef
// 直接改写 textContent，进度条 scaleX 由三个时间线分段驱动。

import React from 'react';
import { APP_VERSION_LABEL } from '../../core/domain/app-meta';
import { SUB_TEXT } from '../../core/domain/bootstrap/splashSpec';
import type { SplashRefs } from './splashRefs';

export const SplashStatus: React.FC<{ refs: SplashRefs }> = ({ refs }) => (
    <>
        <div className="flex flex-col items-center gap-1.5 text-center">
            <h1
                ref={refs.titleRef}
                className="bg-gradient-to-r from-text via-brand-200 to-text bg-clip-text text-xl font-semibold tracking-tight text-transparent drop-shadow-sm"
            >
                NapCatQQ Desktop
            </h1>
            <div ref={refs.subRef} className="flex items-center gap-2 text-sm text-text-secondary">
                <span className="ndf-splash-pulse-dot" aria-hidden />
                <span ref={refs.subTextRef}>{SUB_TEXT.wake}</span>
            </div>
        </div>
        <div
            ref={refs.barTrackRef}
            className="relative h-1.5 w-56 overflow-hidden rounded-full bg-border-subtle shadow-inner"
            aria-hidden
        >
            <div
                ref={refs.barRef}
                className="ndf-splash-laser-bar h-full w-full origin-left rounded-full"
            />
            <div
                ref={refs.barShineRef}
                className="ndf-splash-bar-shine pointer-events-none absolute inset-y-0 left-0 w-1/3 rounded-full opacity-0"
            />
        </div>
        <p ref={refs.versionRef} className="text-xs text-text-tertiary tabular-nums">
            {APP_VERSION_LABEL}
        </p>
    </>
);
