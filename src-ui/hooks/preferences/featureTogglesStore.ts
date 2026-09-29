// 功能模块开关（app-settings 的 features）。启动时从磁盘恢复，保存设置后更新；
// 侧栏、标题栏和各页入口只读这里。

import { useSyncExternalStore } from 'react';
import {
    DEFAULT_FEATURES,
    featuresEqual,
    normalizeFeatures,
    type FeatureKey,
    type FeatureToggles,
} from '../../core/domain/settings/features';
import { createStore } from '../utils/createStore';

const store = createStore<FeatureToggles>({ ...DEFAULT_FEATURES });

export const featureTogglesStore = {
    getSnapshot: store.getSnapshot,
    subscribe: store.subscribe,
    apply(next: Partial<FeatureToggles> | null | undefined): void {
        const normalized = normalizeFeatures(next);
        if (featuresEqual(normalized, store.getSnapshot())) return;
        store.setState(normalized);
    },
    _reset: store._reset,
};

export function useFeatures(): FeatureToggles {
    return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}

export function useFeatureEnabled(key: FeatureKey): boolean {
    return useFeatures()[key];
}
