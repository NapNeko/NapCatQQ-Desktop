// SnowLuma 聚合 hook —— 现在只是模块级 store 的 useSyncExternalStore 视图。
// 路由切换组件 mount/unmount 不影响 state，事件订阅启动一次后台累积。

import { useSyncExternalStore } from 'react';
import { snowlumaStore } from './snowlumaStore';
import type { SnowlumaState } from '../../core/domain/events/snowluma-aggregator';

export function useSnowlumaState(): SnowlumaState {
    return useSyncExternalStore(snowlumaStore.subscribe, snowlumaStore.getSnapshot);
}
