import { afterEach, describe, expect, it, vi } from 'vitest';
import {
    _resetDebugNavForTests,
    consumePendingDebugBot,
    getPendingDebugBot,
    openDebugConsole,
    registerDebugNavigator,
    subscribePendingDebugBot,
} from './debugNav';

afterEach(() => {
    _resetDebugNavForTests();
});

describe('debugNav', () => {
    it('打开时先记下待选 Bot 再触发跳转，consume 取一次就清空', () => {
        const order: string[] = [];
        registerDebugNavigator(() => order.push(`nav:${getPendingDebugBot()}`));

        openDebugConsole('bot-1');

        expect(order).toEqual(['nav:bot-1']);
        expect(consumePendingDebugBot()).toBe('bot-1');
        expect(consumePendingDebugBot()).toBeNull();
    });

    it('页面还没挂载（没注册跳转）时待选 Bot 也留着，挂载后能取到', () => {
        openDebugConsole('bot-2');
        expect(getPendingDebugBot()).toBe('bot-2');
        expect(consumePendingDebugBot()).toBe('bot-2');
    });

    it('不带 Bot 打开会清掉没取走的旧目标', () => {
        const nav = vi.fn();
        registerDebugNavigator(nav);
        openDebugConsole('bot-1');
        openDebugConsole();
        expect(nav).toHaveBeenCalledTimes(2);
        expect(getPendingDebugBot()).toBeNull();
    });

    it('已挂载的页面靠订阅得知待选 Bot 变化；取走后也通知', () => {
        const seen: Array<string | null> = [];
        const off = subscribePendingDebugBot(() => seen.push(getPendingDebugBot()));

        openDebugConsole('bot-3');
        openDebugConsole('bot-3'); // 同一个目标不重复通知
        consumePendingDebugBot();
        off();
        openDebugConsole('bot-4');

        expect(seen).toEqual(['bot-3', null]);
    });

    it('注销只撤自己注册的那份', () => {
        const first = vi.fn();
        const second = vi.fn();
        const offFirst = registerDebugNavigator(first);
        registerDebugNavigator(second);
        offFirst();
        openDebugConsole();
        expect(first).not.toHaveBeenCalled();
        expect(second).toHaveBeenCalledTimes(1);
    });
});
