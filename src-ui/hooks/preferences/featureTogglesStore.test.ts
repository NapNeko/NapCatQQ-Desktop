import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook } from '@testing-library/react';
import { featureTogglesStore, useFeatureEnabled } from './featureTogglesStore';

afterEach(() => {
    cleanup();
    featureTogglesStore._reset();
});

describe('featureTogglesStore', () => {
    it('单功能订阅只在选中开关变化时刷新，切换订阅目标时读取最新值', () => {
        const render = vi.fn();
        const { result, rerender } = renderHook(
            ({ feature }: { feature: 'chat' | 'terminal' }) => {
                render();
                return useFeatureEnabled(feature);
            },
            { initialProps: { feature: 'chat' } },
        );
        expect(result.current).toBe(true);
        act(() => featureTogglesStore.apply({ terminal: false }));
        expect(render).toHaveBeenCalledTimes(1);
        act(() => featureTogglesStore.apply({ terminal: false, chat: false }));
        expect(result.current).toBe(false);
        expect(render).toHaveBeenCalledTimes(2);
        act(() => featureTogglesStore.apply({ terminal: true, chat: false }));
        rerender({ feature: 'terminal' });
        expect(result.current).toBe(true);
    });

    it('apply 归一化并通知', () => {
        const listener = vi.fn();
        const unsub = featureTogglesStore.subscribe(listener);
        featureTogglesStore.apply({ apps: false });
        expect(featureTogglesStore.getSnapshot()).toMatchObject({
            apps: false,
            dockerPage: true,
            terminal: true,
        });
        expect(listener).toHaveBeenCalledTimes(1);
        unsub();
    });

    it('值没变不通知', () => {
        const listener = vi.fn();
        const unsub = featureTogglesStore.subscribe(listener);
        featureTogglesStore.apply({});
        expect(listener).not.toHaveBeenCalled();
        unsub();
    });

    it('保存聊天开关后通知入口，重新开启不改变其它模块', () => {
        const listener = vi.fn();
        const unsub = featureTogglesStore.subscribe(listener);
        featureTogglesStore.apply({ chat: false, apiDebug: false });
        expect(featureTogglesStore.getSnapshot().chat).toBe(false);
        featureTogglesStore.apply({ ...featureTogglesStore.getSnapshot(), chat: true });
        expect(featureTogglesStore.getSnapshot()).toMatchObject({ chat: true, apiDebug: false });
        expect(listener).toHaveBeenCalledTimes(2);
        unsub();
    });
});
