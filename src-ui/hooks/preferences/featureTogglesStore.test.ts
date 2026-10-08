import { afterEach, describe, expect, it, vi } from 'vitest';
import { featureTogglesStore } from './featureTogglesStore';

afterEach(() => featureTogglesStore._reset());

describe('featureTogglesStore', () => {
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
