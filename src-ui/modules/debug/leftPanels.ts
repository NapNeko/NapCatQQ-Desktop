// 左栏的三个面板。展开时是标题行里的分段切换，收成窄边时是一列图标，两处用同一份定义。
//
// 底下还挂着「这个面板的搜索条开没开」：搜索条默认收成标题行里的一个图标按钮（分段切换旁），
// 点图标、按 /、在列表里直接打字才展开。开关状态纯界面、不落盘，切面板 / 切路由再回来还在。

import { useSyncExternalStore } from 'react';
import { History, ListTree, Star, type LucideIcon } from 'lucide-react';

export type DebugLeftPanel = 'catalog' | 'collections' | 'history';

export const LEFT_PANELS: ReadonlyArray<{
    id: DebugLeftPanel;
    label: string;
    hint: string;
    icon: LucideIcon;
    /** 搜索条的名字（图标按钮的 aria-label 和提示）；没写的面板不出搜索按钮 */
    searchLabel?: string;
}> = [
    { id: 'catalog', label: '接口', hint: '接口目录', icon: ListTree, searchLabel: '搜索接口' },
    { id: 'collections', label: '收藏', hint: '收藏的请求', icon: Star },
    { id: 'history', label: '历史', hint: '调用历史', icon: History, searchLabel: '搜索历史' },
];

// ---------------------------------------------------------------------------
// 搜索条的开关
// ---------------------------------------------------------------------------

interface LeftSearchState {
    open: boolean;
    /** 「打开并聚焦」的凭据：点图标、按 / 时 +1，面板看到变了就聚焦输入框。
     *  面板挂载时自己把条同步开（搜索词还在）不碰它，不抢焦点 */
    focusNonce: number;
}

const searchState: Record<DebugLeftPanel, LeftSearchState> = {
    catalog: { open: false, focusNonce: 0 },
    collections: { open: false, focusNonce: 0 },
    history: { open: false, focusNonce: 0 },
};
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

export function setLeftSearchOpen(panel: DebugLeftPanel, open: boolean): void {
    if (searchState[panel].open === open) return;
    searchState[panel] = { ...searchState[panel], open };
    emit();
}

/** 打开搜索条并请求聚焦输入框（点图标、按 /、在列表里直接打字） */
export function revealLeftSearch(panel: DebugLeftPanel): void {
    searchState[panel] = { open: true, focusNonce: searchState[panel].focusNonce + 1 };
    emit();
}

export function useLeftSearch(panel: DebugLeftPanel): LeftSearchState {
    return useSyncExternalStore(
        (cb) => {
            listeners.add(cb);
            return () => listeners.delete(cb);
        },
        () => searchState[panel],
    );
}
