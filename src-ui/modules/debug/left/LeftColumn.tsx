// 左栏内容：接口目录 / 收藏 / 历史三个面板之一。
//
// 外框（标题行里的三段切换、收起按钮、收起后的窄边、错误边界）由页面画，这里只画当前面板本身，
// 高度由外框给定，列表在里面自己滚。切换面板时内容朝切换方向轻轻滑入（只动 transform / opacity）；
// 各面板的搜索词、筛选、折叠状态和滚动位置都记在模块里，切走再切回来还在。

import { memo, useLayoutEffect, useRef } from 'react';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { cssEase } from '../../../core/design/cssEase';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { LEFT_PANELS, type DebugLeftPanel } from '../leftPanels';
import { CatalogPanel } from './CatalogPanel';
import { CollectionsPanel } from './CollectionsPanel';
import { HistoryPanel } from './HistoryPanel';

export interface LeftColumnProps {
    /** 当前选中的 Bot：目录按它的后端和在线状态取（useDebugCatalog 可以直接收它）；没有选中时是 null */
    target: DebugTarget | null;
    /** 显示哪个面板；切换控件在外框标题行里 */
    panel: DebugLeftPanel;
}

const panelIndex = (p: DebugLeftPanel) => LEFT_PANELS.findIndex((x) => x.id === p);

export const LeftColumn = memo(function LeftColumn({ target, panel }: LeftColumnProps) {
    const m = useMotion();
    const ref = useRef<HTMLDivElement>(null);
    const prevPanel = useRef(panel);

    useLayoutEffect(() => {
        const from = prevPanel.current;
        prevPanel.current = panel;
        const el = ref.current;
        // 首次挂上不播：展开整栏时外框已经有滑入动画了
        if (from === panel || !el || !m.enabled || typeof el.animate !== 'function') return;
        const dx = panelIndex(panel) > panelIndex(from) ? 8 : -8;
        const anim = el.animate(
            [
                { opacity: 0, transform: `translateX(${dx}px)` },
                { opacity: 1, transform: 'none' },
            ],
            { duration: m.duration('fast') * 1000, easing: cssEase(m.ease.enter) },
        );
        return () => anim.cancel();
    }, [panel, m]);

    return (
        <div ref={ref} data-debug-left-panel={panel} className="flex min-h-0 min-w-0 flex-1 flex-col">
            {panel === 'catalog' ? (
                <CatalogPanel target={target} />
            ) : panel === 'collections' ? (
                <CollectionsPanel target={target} />
            ) : (
                <HistoryPanel target={target} />
            )}
        </div>
    );
});
