// 订阅全局 InfoBar 队列 + 暴露 push / dismiss 给 React 组件用。
//
// AppNext.tsx 顶层调一次 useGlobalInfoBars() 拿 bars + dismiss 渲染 InfoBarStack。
// 各页面 hook 想推 banner 时调 const { push } = useGlobalInfoBars()，或者直接
// import { pushInfoBar } from './globalInfoBarStore'（非 React 上下文友好）。

import { useCallback, useSyncExternalStore } from 'react';
import {
    globalInfoBarStore,
    type PushInfoBarOptions,
} from './globalInfoBarStore';
import type { InfoBarStackItem } from '../../shared/ui';

export interface UseGlobalInfoBarsResult {
    bars: InfoBarStackItem[];
    push: (opts: PushInfoBarOptions) => string;
    dismiss: (id: string) => void;
    remove: (id: string) => void;
}

export function useGlobalInfoBars(): UseGlobalInfoBarsResult {
    const state = useSyncExternalStore(
        globalInfoBarStore.subscribe,
        globalInfoBarStore.getSnapshot,
        globalInfoBarStore.getSnapshot,
    );

    const push = useCallback((opts: PushInfoBarOptions) => {
        return globalInfoBarStore.push(opts);
    }, []);

    const dismiss = useCallback((id: string) => {
        globalInfoBarStore.dismiss(id);
    }, []);

    const remove = useCallback((id: string) => {
        globalInfoBarStore.remove(id);
    }, []);

    return { bars: state.bars, push, dismiss, remove };
}
