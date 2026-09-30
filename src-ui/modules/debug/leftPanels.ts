// 左栏的三个面板。展开时是标题行里的分段切换，收成窄边时是一列图标，两处用同一份定义。

import { History, ListTree, Star, type LucideIcon } from 'lucide-react';

export type DebugLeftPanel = 'catalog' | 'collections' | 'history';

export const LEFT_PANELS: ReadonlyArray<{ id: DebugLeftPanel; label: string; hint: string; icon: LucideIcon }> = [
    { id: 'catalog', label: '接口', hint: '接口目录', icon: ListTree },
    { id: 'collections', label: '收藏', hint: '收藏的请求', icon: Star },
    { id: 'history', label: '历史', hint: '调用历史', icon: History },
];
