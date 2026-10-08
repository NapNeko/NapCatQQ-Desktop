import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
    collectFrontendPreferences,
    onFrontendPreferenceRestored,
    restoreFrontendPreferences,
    validateFrontendPreferences,
} from './config-transfer-preferences';

const NAME_KEY = 'ncd.maibot.chat.name.10001';
const TERM_KEY = 'ncd.terminal.prefs.v1';
const ORDER_KEY = 'ncd:bot_custom_order:v1';
const CHAT_KEY = 'ncd.chat.ui.v1';
const CHAT_OLD = '{"listWidth":200}';
const CHAT_NEW = '{"listWidth":260}';

/** 满足 Storage 结构的极简内存实现（可选让某个键的 setItem 抛错，模拟配额） */
function makeStorage(init: Record<string, string> = {}, failOnSetKey?: string) {
    const map = new Map(Object.entries(init));
    return {
        get length() {
            return map.size;
        },
        key: (index: number) => [...map.keys()][index] ?? null,
        getItem: (key: string) => map.get(key) ?? null,
        setItem: (key: string, value: string) => {
            if (key === failOnSetKey) throw new Error('QuotaExceeded');
            map.set(key, value);
        },
        removeItem: (key: string) => {
            map.delete(key);
        },
        clear: () => map.clear(),
        dump: () => Object.fromEntries(map),
    };
}

beforeEach(() => {
    window.localStorage.clear();
});

afterEach(() => {
    window.localStorage.clear();
});

describe('validateFrontendPreferences', () => {
    it('合法快照通过：JSON 键 + 昵称键混合', () => {
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: {
                    [TERM_KEY]: JSON.stringify({
                        fontSize: 14,
                        cursorStyle: 'block',
                        defaultShell: 'pwsh',
                        snippets: [{ label: '重启', command: 'pm2 restart napcat' }],
                    }),
                    [CHAT_KEY]: JSON.stringify({ listWidth: 260, composerHeight: null }),
                    [ORDER_KEY]: JSON.stringify(['10001', '10002']),
                    [NAME_KEY]: '小明',
                },
            } as unknown),
        ).not.toThrow();
    });

    it('版本 / 顶层结构不对直接拒', () => {
        expect(() => validateFrontendPreferences({ version: 2, storage: {} })).toThrow();
        expect(() => validateFrontendPreferences({ version: 1 })).toThrow();
        expect(() => validateFrontendPreferences([])).toThrow();
        expect(() => validateFrontendPreferences(null)).toThrow();
    });

    it('白名单外的键拒收，昵称后缀非法也拒收', () => {
        expect(() =>
            validateFrontendPreferences({ version: 1, storage: { 'ncd.other.v1': '{}' } }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: { 'ncd.maibot.chat.name.': 'x' },
            }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: { 'ncd.maibot.chat.name.有中文': 'x' },
            }),
        ).toThrow();
    });

    it('值必须是字符串、单值不超 512KB、昵称值不超 128 字符且不含 NUL', () => {
        expect(() =>
            validateFrontendPreferences({ version: 1, storage: { [NAME_KEY]: 42 } }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: { [NAME_KEY]: 'x'.repeat(512 * 1024 + 1) },
            }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({ version: 1, storage: { [NAME_KEY]: 'x'.repeat(129) } }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({ version: 1, storage: { [NAME_KEY]: 'a\0b' } }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({ version: 1, storage: { [NAME_KEY]: 'x'.repeat(128) } }),
        ).not.toThrow();
    });

    it('条目数超 512 或总量超 2MB 拒收', () => {
        const tooMany: Record<string, string> = {};
        for (let i = 0; i < 513; i += 1) tooMany[`ncd.maibot.chat.name.k${i}`] = 'x';
        expect(() => validateFrontendPreferences({ version: 1, storage: tooMany })).toThrow();

        const tooBig: Record<string, string> = {};
        for (let i = 0; i < 5; i += 1)
            tooBig[`ncd.maibot.chat.name.k${i}`] = 'x'.repeat(500 * 1024);
        expect(() => validateFrontendPreferences({ version: 1, storage: tooBig })).toThrow();
    });

    it('JSON 键的值必须可解析且符合各自结构', () => {
        expect(() =>
            validateFrontendPreferences({ version: 1, storage: { [TERM_KEY]: 'not json' } }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({ version: 1, storage: { [TERM_KEY]: '[1,2]' } }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: { [TERM_KEY]: JSON.stringify({ fontSize: '14' }) },
            }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: { [TERM_KEY]: JSON.stringify({ cursorStyle: 'italic' }) },
            }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: { [TERM_KEY]: JSON.stringify({ snippets: [{ label: 'ok' }] }) },
            }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: { [ORDER_KEY]: JSON.stringify(['10001', '']) },
            }),
        ).toThrow();
    });

    it('聊天界面偏好：composerHeight 允许 null，hiddenConversations 有形状约束', () => {
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: { [CHAT_KEY]: JSON.stringify({ composerHeight: null }) },
            }),
        ).not.toThrow();
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: { [CHAT_KEY]: JSON.stringify({ composerHeight: '30' }) },
            }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: {
                    [CHAT_KEY]: JSON.stringify({
                        hiddenConversations: { 'group:10001': [123] },
                    }),
                },
            }),
        ).toThrow();
        expect(() =>
            validateFrontendPreferences({
                version: 1,
                storage: {
                    [CHAT_KEY]: JSON.stringify({
                        hiddenConversations: { 'group:10001': ['file:///a.png'] },
                    }),
                },
            }),
        ).not.toThrow();
    });
});

describe('collectFrontendPreferences', () => {
    it('只收白名单键，忽略其余', () => {
        window.localStorage.setItem(CHAT_KEY, '{"listWidth":260}');
        window.localStorage.setItem(NAME_KEY, '小明');
        window.localStorage.setItem('ncd.session.token', 'secret');
        window.localStorage.setItem('ncd.maibot.chat.name.bad suffix!', 'skip');
        const snapshot = collectFrontendPreferences();
        expect(snapshot).toEqual({
            version: 1,
            storage: { [CHAT_KEY]: '{"listWidth":260}', [NAME_KEY]: '小明' },
        });
    });

    it('内容不合法时拒绝打包', () => {
        window.localStorage.setItem(TERM_KEY, 'not json');
        expect(() => collectFrontendPreferences()).toThrow();
    });

    it('可以指向别的存储', () => {
        const storage = makeStorage({ [ORDER_KEY]: '["10001"]', 'ncd.other': 'x' });
        expect(collectFrontendPreferences(storage as unknown as Storage)).toEqual({
            version: 1,
            storage: { [ORDER_KEY]: '["10001"]' },
        });
    });
});

describe('restoreFrontendPreferences', () => {
    it('写入快照、清掉快照里没有的白名单旧键', () => {
        window.localStorage.setItem(CHAT_KEY, CHAT_OLD);
        window.localStorage.setItem(ORDER_KEY, '["99999"]');
        window.localStorage.setItem('ncd.session.token', 'secret');
        restoreFrontendPreferences(
            { version: 1, storage: { [CHAT_KEY]: CHAT_NEW } },
            window.localStorage,
        );
        expect(window.localStorage.getItem(CHAT_KEY)).toBe(CHAT_NEW);
        expect(window.localStorage.getItem(ORDER_KEY)).toBeNull();
        expect(window.localStorage.getItem('ncd.session.token')).toBe('secret');
    });

    it('写入中途失败按原样回滚并把原因带出来', () => {
        const storage = makeStorage({ [CHAT_KEY]: CHAT_OLD, [ORDER_KEY]: '["10001"]' }, TERM_KEY);
        expect(() =>
            restoreFrontendPreferences(
                { version: 1, storage: { [CHAT_KEY]: CHAT_NEW, [TERM_KEY]: '{"fontSize":14}' } },
                storage as unknown as Storage,
            ),
        ).toThrow('QuotaExceeded');
        const dump = storage.dump();
        expect(dump[CHAT_KEY]).toBe(CHAT_OLD);
        expect(dump[ORDER_KEY]).toBe('["10001"]');
        expect(dump[TERM_KEY]).toBeUndefined();
    });

    it('非法快照在动手前就拒', () => {
        const storage = makeStorage({ [CHAT_KEY]: CHAT_OLD });
        expect(() =>
            restoreFrontendPreferences(
                { version: 1, storage: { 'ncd.evil.v1': 'x' } },
                storage as unknown as Storage,
            ),
        ).toThrow();
        expect(storage.dump()[CHAT_KEY]).toBe(CHAT_OLD);
    });

    it('只有写回真实 localStorage 才广播恢复', () => {
        const spy = vi.fn();
        window.addEventListener('ncd:config-preferences-restored', spy);
        const fake = makeStorage();
        restoreFrontendPreferences(
            { version: 1, storage: { [CHAT_KEY]: CHAT_NEW } },
            fake as unknown as Storage,
        );
        expect(spy).not.toHaveBeenCalled();
        restoreFrontendPreferences({ version: 1, storage: { [CHAT_KEY]: CHAT_NEW } });
        expect(spy).toHaveBeenCalledTimes(1);
        window.removeEventListener('ncd:config-preferences-restored', spy);
    });
});

describe('onFrontendPreferenceRestored', () => {
    it('同窗口广播只叫醒对应键的订阅', () => {
        const cb = vi.fn();
        const unsub = onFrontendPreferenceRestored(CHAT_KEY, cb);
        restoreFrontendPreferences({ version: 1, storage: { [CHAT_KEY]: CHAT_NEW } });
        expect(cb).toHaveBeenCalledTimes(1);
        restoreFrontendPreferences({ version: 1, storage: { [TERM_KEY]: '{"fontSize":14}' } });
        expect(cb).toHaveBeenCalledTimes(1);
        unsub();
        restoreFrontendPreferences({ version: 1, storage: { [CHAT_KEY]: '{"listWidth":240}' } });
        expect(cb).toHaveBeenCalledTimes(1);
    });

    it('其他标签页的 storage 事件同样触发，键或区域不符不触发', () => {
        const cb = vi.fn();
        const unsub = onFrontendPreferenceRestored(NAME_KEY, cb);
        window.dispatchEvent(
            new StorageEvent('storage', { key: NAME_KEY, storageArea: window.localStorage }),
        );
        expect(cb).toHaveBeenCalledTimes(1);
        window.dispatchEvent(new StorageEvent('storage', { key: NAME_KEY }));
        expect(cb).toHaveBeenCalledTimes(1);
        window.dispatchEvent(
            new StorageEvent('storage', { key: 'ncd.other', storageArea: window.localStorage }),
        );
        expect(cb).toHaveBeenCalledTimes(1);
        unsub();
        window.dispatchEvent(
            new StorageEvent('storage', { key: NAME_KEY, storageArea: window.localStorage }),
        );
        expect(cb).toHaveBeenCalledTimes(1);
    });
});
