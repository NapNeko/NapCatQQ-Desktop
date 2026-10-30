// 主窗、聊天窗、调试窗和启动层共用；不改变业务布局或浮层挂载位置。
import { useLayoutEffect, type CSSProperties, type MouseEvent, type ReactNode } from 'react';
import {
    WindowControlsProvider,
    useWindowControls,
} from '../../../hooks/desktop/useWindowControls';
import { isTauri, localOs } from '../../../core/domain/runtime/env';
import { BodyPortal } from '../../ui/BodyPortal';

const RESIZE_REGIONS = [
    ['North', { top: 0, left: 12, right: 12, height: 4, cursor: 'ns-resize' }],
    ['South', { bottom: 0, left: 12, right: 12, height: 4, cursor: 'ns-resize' }],
    ['West', { left: 0, top: 12, bottom: 12, width: 4, cursor: 'ew-resize' }],
    ['East', { right: 0, top: 12, bottom: 12, width: 4, cursor: 'ew-resize' }],
    ['NorthWest', { left: 0, top: 0, width: 12, height: 12, cursor: 'nwse-resize' }],
    ['NorthEast', { right: 0, top: 0, width: 12, height: 12, cursor: 'nesw-resize' }],
    ['SouthWest', { left: 0, bottom: 0, width: 12, height: 12, cursor: 'nesw-resize' }],
    ['SouthEast', { right: 0, bottom: 0, width: 12, height: 12, cursor: 'nwse-resize' }],
] as const satisfies ReadonlyArray<readonly [string, CSSProperties]>;

function WindowEdges() {
    const { ready, isMaximized, isFullscreen, isResizable, startResizeDragging } =
        useWindowControls();
    const expanded = isMaximized || isFullscreen;
    useLayoutEffect(() => {
        if (!isTauri) return;
        document.documentElement.toggleAttribute('data-window-expanded', expanded);
        return () => document.documentElement.removeAttribute('data-window-expanded');
    }, [expanded]);

    if (!isTauri || localOs() === 'mac_os' || !ready || expanded || !isResizable) return null;
    return (
        <BodyPortal>
            {/* body 可被 Radix modal 设为 pointer-events:none，边缘须显式恢复命中。 */}
            <div className="ndf-window-resize-handles" aria-hidden="true">
                {RESIZE_REGIONS.map(([direction, style]) => (
                    <div
                        key={direction}
                        data-window-resize={direction}
                        style={style}
                        onPointerDown={(event) => event.stopPropagation()}
                        // 与 Tauri 拖动区域使用同一鼠标事件路径；GTK 原生拖动会接管松键事件。
                        onMouseDown={(event: MouseEvent<HTMLDivElement>) => {
                            if (event.button !== 0) return;
                            event.preventDefault();
                            event.stopPropagation();
                            startResizeDragging(direction);
                        }}
                    />
                ))}
            </div>
        </BodyPortal>
    );
}

export function WindowFrame({ children }: { children: ReactNode }) {
    return (
        <WindowControlsProvider>
            {children}
            <WindowEdges />
        </WindowControlsProvider>
    );
}
