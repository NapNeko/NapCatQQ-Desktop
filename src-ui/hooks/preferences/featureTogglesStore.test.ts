import { afterEach, describe, expect, it, vi } from 'vitest';
import { featureTogglesStore } from './featureTogglesStore';

afterEach(() => featureTogglesStore._reset());

describe('featureTogglesStore', () => {
    it('apply 归一化并通知', () => {
        const listener = vi.fn();
        const unsub = featureTogglesStore.subscribe(listener);
        featureTogglesStore.apply({ apps: false });
        expect(featureTogglesStore.getSnapshot()).toMatchObject({ apps: false, dockerPage: true, terminal: true });
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
});
