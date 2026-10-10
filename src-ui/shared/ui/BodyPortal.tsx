// 把 fixed 浮层挂出去，避免祖先 overflow/transform 裁切或错位。
// BodyPortal 挂 document.body；FloatLayerPortal 挂主壳里的浮层槽，压在终端面板之下。

import { createPortal } from 'react-dom';
import type { ReactNode } from 'react';

interface BodyPortalProps {
    children: ReactNode;
    enabled?: boolean;
}

export function BodyPortal({ children, enabled = true }: BodyPortalProps) {
    if (!enabled || typeof document === 'undefined') {
        return <>{children}</>;
    }
    return createPortal(children, document.body);
}

/** AppNext 主内容区里的浮层槽 id；和终端面板同一层叠上下文，面板 z 更高。 */
export const FLOAT_LAYER_ID = 'ndf-float-layer';

/** 页面贴底悬浮按钮 / 操作条用：跟页面内容一样被终端面板盖住，而不是浮在面板上。
 *  渲染期同步查槽（不延后一帧，否则调用方 mount 时的进场动画拿不到 ref）；页面是懒加载的，
 *  到这里槽早已挂上。没有槽（测试、独立窗口）时退回 body。 */
export function FloatLayerPortal({ children }: { children: ReactNode }) {
    if (typeof document === 'undefined') return <>{children}</>;
    return createPortal(children, document.getElementById(FLOAT_LAYER_ID) ?? document.body);
}
