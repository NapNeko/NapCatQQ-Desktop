// 启动前要用户（重新）同意上游条款时弹的框（MaiBot 的 EULA / 隐私条款，更新后哈希变了就得重新同意）。
// 启动在 hook 里，没法直接渲染对话框：走模块级 store + 单一宿主，用 Promise 把「同意 / 取消」交回启动流程。

import { useSyncExternalStore } from 'react';
import { createStore } from '../utils/createStore';
import type { AppPendingTerms } from '../../core/ipc/types';

export interface TermsDialogState {
    instanceName: string;
    terms: AppPendingTerms[];
    settle: (accepted: boolean) => void;
}

const store = createStore<TermsDialogState | null>(null);

export function requestTermsConsent(
    instanceName: string,
    terms: AppPendingTerms[],
): Promise<boolean> {
    // 上一个框还没答就又来一个：旧的当取消，免得留下永远不回的 Promise
    store.getSnapshot()?.settle(false);
    return new Promise((resolve) => {
        store.setState({
            instanceName,
            terms,
            settle: (accepted) => {
                store.setState(null);
                resolve(accepted);
            },
        });
    });
}

export function useTermsDialog(): TermsDialogState | null {
    return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}

/** 不经 React 读当前框（测试用） */
export function peekTermsDialog(): TermsDialogState | null {
    return store.getSnapshot();
}

/** 测试用 */
export function _resetTermsDialog(): void {
    store._reset();
}
