// preferencesStore 的 React 视图。store 本体在 core/domain（零框架依赖），
// 订阅/渲染这层放 hooks 层。

import { useSyncExternalStore } from 'react';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import type { AppPreferences } from '../../core/domain/settings/appPreferences';

/// 组件 mount 时同步 store 当前快照，store 变化时重新渲染。
export function usePreferences(): AppPreferences {
    return useSyncExternalStore(
        (l) => preferencesStore.subscribe(l),
        () => preferencesStore.get(),
        () => preferencesStore.get(),
    );
}
