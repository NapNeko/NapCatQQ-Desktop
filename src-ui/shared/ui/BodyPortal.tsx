// 把 fixed 浮层挂到 document.body，避免祖先 overflow/transform 裁切或错位。
// InfoBarStack 已自带 portal；Bot 列表 FAB / 批量底栏等复用本组件。

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