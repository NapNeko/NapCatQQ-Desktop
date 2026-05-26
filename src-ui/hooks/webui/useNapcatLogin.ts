// NapCat 登录态聚合 hook —— 现在只是模块级 store 的 useSyncExternalStore 视图。
// 路由切换组件 mount/unmount 不会影响 state；订阅启动一次，后台累积事件不丢。

import { useSyncExternalStore } from 'react';
import { napcatLoginStore } from './napcatLoginStore';
import type { NapcatLoginState } from '../../core/domain/events/login-aggregator';

export function useNapcatLogin(): NapcatLoginState {
    return useSyncExternalStore(napcatLoginStore.subscribe, napcatLoginStore.getSnapshot);
}
