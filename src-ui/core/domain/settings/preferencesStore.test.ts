import { beforeEach, describe, expect, it, vi } from 'vitest';
import type * as PreferencesStore from './preferencesStore';
import { MOTION_SPEED_DEFAULT, MOTION_SPEED_MAX, MOTION_SPEED_MIN } from '../../design/motion';

// 模块级单例 + 加载即读档：换存档必须 resetModules 重新 import 才有干净的 loadFromStorage。
const STORAGE_KEY = 'ncd:preferences:v1';

type Store = typeof PreferencesStore;

async function loadStore(seed?: unknown): Promise<Store> {
    localStorage.clear();
    if (seed !== undefined) {
        localStorage.setItem(STORAGE_KEY, typeof seed === 'string' ? seed : JSON.stringify(seed));
    }
    vi.resetModules();
    return import('./preferencesStore');
}

describe('preferencesStore 读档归一化', () => {
    beforeEach(() => {
        localStorage.clear();
    });

    it('无存档时给默认值', async () => {
        const { preferencesStore } = await loadStore();
        expect(preferencesStore.get()).toEqual({
            theme: 'auto',
            showMascot: true,
            closeAction: 'close',
            motionEnabled: true,
            motionLevel: 'standard',
            motionSpeed: MOTION_SPEED_DEFAULT,
            radiusStyle: 'standard',
        });
    });

    it('坏 JSON 退回默认值而不是崩溃', async () => {
        const { preferencesStore } = await loadStore('{not json');
        expect(preferencesStore.get().theme).toBe('auto');
        expect(preferencesStore.get().motionEnabled).toBe(true);
    });

    it('老存档只有部分字段时逐项补默认', async () => {
        const { preferencesStore } = await loadStore({ theme: 'mocha' });
        const prefs = preferencesStore.get();
        expect(prefs.theme).toBe('mocha');
        expect(prefs.showMascot).toBe(true);
        expect(prefs.closeAction).toBe('close');
        expect(prefs.motionSpeed).toBe(MOTION_SPEED_DEFAULT);
    });

    it('存档里认识的值原样保留', async () => {
        const { preferencesStore } = await loadStore({
            theme: 'dracula',
            showMascot: false,
            closeAction: 'tray',
            motionEnabled: false,
            motionLevel: 'rich',
            motionSpeed: 1.2,
            radiusStyle: 'round',
        });
        expect(preferencesStore.get()).toEqual({
            theme: 'dracula',
            showMascot: false,
            closeAction: 'tray',
            motionEnabled: false,
            motionLevel: 'rich',
            motionSpeed: 1.2,
            radiusStyle: 'round',
        });
    });

    it('未知枚举值逐项兜底', async () => {
        const { preferencesStore } = await loadStore({
            theme: 'not-a-theme',
            closeAction: 'minimize',
            motionLevel: 'ultra',
            radiusStyle: 'pointy',
        });
        const prefs = preferencesStore.get();
        expect(prefs.theme).toBe('auto');
        expect(prefs.closeAction).toBe('close');
        expect(prefs.motionLevel).toBe('standard');
        expect(prefs.radiusStyle).toBe('standard');
    });

    it('motionSpeed 只认有限数并夹到区间内', async () => {
        const cases: Array<[unknown, number]> = [
            ['1.2', MOTION_SPEED_DEFAULT],
            [null, MOTION_SPEED_DEFAULT],
            [99, MOTION_SPEED_MAX],
            [-3, MOTION_SPEED_MIN],
            [MOTION_SPEED_MIN, MOTION_SPEED_MIN],
            [MOTION_SPEED_MAX, MOTION_SPEED_MAX],
        ];
        for (const [raw, want] of cases) {
            const { preferencesStore } = await loadStore({ motionSpeed: raw });
            expect(preferencesStore.get().motionSpeed).toBe(want);
        }
    });

    it('setter 写入后重新加载得到同一状态（往返）', async () => {
        const first = await loadStore();
        first.preferencesStore.applySnapshot({
            theme: 'dark',
            closeAction: 'tray',
            motionLevel: 'elegant',
            motionSpeed: 1.4,
            radiusStyle: 'square',
        });
        const second = await loadStore(localStorage.getItem(STORAGE_KEY));
        expect(second.preferencesStore.get()).toEqual(first.preferencesStore.get());
    });
});

describe('preferencesStore setter 归一化', () => {
    it('setTheme 拒绝未知主题', async () => {
        const { preferencesStore } = await loadStore({ theme: 'dark' });
        preferencesStore.setTheme('bogus' as never);
        expect(preferencesStore.get().theme).toBe('auto');
        preferencesStore.setTheme('mocha');
        expect(preferencesStore.get().theme).toBe('mocha');
    });

    it('setCloseAction / setMotionLevel / setMotionSpeed / setRadiusStyle 走同一套兜底', async () => {
        const { preferencesStore } = await loadStore();
        preferencesStore.setCloseAction('tray');
        expect(preferencesStore.get().closeAction).toBe('tray');
        preferencesStore.setMotionLevel('nope' as never);
        expect(preferencesStore.get().motionLevel).toBe('standard');
        preferencesStore.setMotionSpeed(MOTION_SPEED_MAX + 1);
        expect(preferencesStore.get().motionSpeed).toBe(MOTION_SPEED_MAX);
        preferencesStore.setRadiusStyle('round');
        expect(preferencesStore.get().radiusStyle).toBe('round');
    });

    it('applySnapshot 只改草稿里出现的字段，其余保留', async () => {
        const { preferencesStore } = await loadStore({
            theme: 'dark',
            closeAction: 'tray',
            motionSpeed: 1.4,
        });
        preferencesStore.applySnapshot({ showMascot: false });
        const prefs = preferencesStore.get();
        expect(prefs.showMascot).toBe(false);
        expect(prefs.theme).toBe('dark');
        expect(prefs.closeAction).toBe('tray');
        expect(prefs.motionSpeed).toBe(1.4);
    });

    it('applySnapshot 里脏值同样被归一化', async () => {
        const { preferencesStore } = await loadStore();
        preferencesStore.applySnapshot({
            theme: 'x' as never,
            closeAction: 'whatever' as never,
            motionSpeed: Number.POSITIVE_INFINITY,
        });
        const prefs = preferencesStore.get();
        expect(prefs.theme).toBe('auto');
        expect(prefs.closeAction).toBe('close');
        expect(prefs.motionSpeed).toBe(MOTION_SPEED_DEFAULT);
    });

    it('reset 回到出厂值', async () => {
        const { preferencesStore } = await loadStore({ theme: 'dracula', closeAction: 'tray' });
        preferencesStore.reset();
        expect(preferencesStore.get()).toEqual({
            theme: 'auto',
            showMascot: true,
            closeAction: 'close',
            motionEnabled: true,
            motionLevel: 'standard',
            motionSpeed: MOTION_SPEED_DEFAULT,
            radiusStyle: 'standard',
        });
    });

    it('订阅者在状态变化时收到通知，退订后不再收到', async () => {
        const { preferencesStore } = await loadStore();
        const seen: number[] = [];
        let n = 0;
        const unsubA = preferencesStore.subscribe(() => {
            n += 1;
            seen.push(n);
        });
        preferencesStore.subscribe(() => undefined);
        preferencesStore.setShowMascot(false);
        preferencesStore.setMotionEnabled(false);
        expect(n).toBe(2);
        unsubA();
        preferencesStore.setShowMascot(true);
        expect(n).toBe(2);
    });
});
