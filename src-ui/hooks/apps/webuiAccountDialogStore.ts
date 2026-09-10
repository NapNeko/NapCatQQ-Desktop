// 打开账号密码类 WebUI 时要弹的账号框。openWebUi 在 hook 里，没法直接渲染对话框，走模块级 store + 单一宿主。

import { useSyncExternalStore } from 'react';
import { createStore } from '../utils/createStore';
import type { AppWebUiAccount } from '../../core/ipc/types';

export interface WebUiAccountDialogState {
    instanceId: string;
    instanceName: string;
    url: string;
    account: AppWebUiAccount;
}

const store = createStore<WebUiAccountDialogState | null>(null);

export function showWebUiAccountDialog(next: WebUiAccountDialogState): void {
    store.setState(next);
}

export function closeWebUiAccountDialog(): void {
    store.setState(null);
}

export function useWebUiAccountDialog(): WebUiAccountDialogState | null {
    return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}
