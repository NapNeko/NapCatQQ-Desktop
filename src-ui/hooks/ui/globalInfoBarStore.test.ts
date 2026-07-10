import { afterEach, describe, expect, it, vi } from 'vitest';
import { globalInfoBarStore } from './globalInfoBarStore';

afterEach(() => {
    globalInfoBarStore._reset();
});

describe('globalInfoBarStore 同 key 去重', () => {
    it('内容不变的重复 push 不触发 emit（防 ComponentsPage 重渲染循环）', () => {
        const listener = vi.fn();
        const unsub = globalInfoBarStore.subscribe(listener);

        const opts = {
            key: 'component-detect:qq:remote:abc',
            tone: 'danger' as const,
            title: 'QQ · 服务器 · 探测失败',
            content: '无法探测远端 $HOME',
        };

        globalInfoBarStore.push(opts);
        expect(listener).toHaveBeenCalledTimes(1);

        // 同 key 同内容再 push 多次：不应再 emit
        globalInfoBarStore.push(opts);
        globalInfoBarStore.push(opts);
        globalInfoBarStore.push(opts);
        expect(listener).toHaveBeenCalledTimes(1);

        unsub();
    });

    it('内容变化的同 key push 仍刷新并 emit', () => {
        const listener = vi.fn();
        const unsub = globalInfoBarStore.subscribe(listener);

        const key = 'component-detect:qq:remote:abc';
        globalInfoBarStore.push({ key, tone: 'danger', title: 'T', content: '原因 A' });
        expect(listener).toHaveBeenCalledTimes(1);

        globalInfoBarStore.push({ key, tone: 'danger', title: 'T', content: '原因 B' });
        expect(listener).toHaveBeenCalledTimes(2);

        // 顶替而非堆叠：始终只有一条
        expect(globalInfoBarStore.getSnapshot().bars).toHaveLength(1);
        expect(globalInfoBarStore.getSnapshot().bars[0].content).toBe('原因 B');

        unsub();
    });

    it('不同 key 各自独立堆叠', () => {
        globalInfoBarStore.push({ key: 'a', tone: 'danger', title: 'A' });
        globalInfoBarStore.push({ key: 'b', tone: 'danger', title: 'B' });
        expect(globalInfoBarStore.getSnapshot().bars).toHaveLength(2);
    });

    it('用户关闭触发抑制回调，程序移除不触发', () => {
        const onUserDismiss = vi.fn();
        globalInfoBarStore.push({
            key: 'recoverable',
            tone: 'danger',
            title: '暂时失败',
            onUserDismiss,
        });

        globalInfoBarStore.remove('key:recoverable');
        expect(onUserDismiss).not.toHaveBeenCalled();

        globalInfoBarStore.push({
            key: 'recoverable',
            tone: 'danger',
            title: '暂时失败',
            onUserDismiss,
        });
        globalInfoBarStore.dismiss('key:recoverable');
        expect(onUserDismiss).toHaveBeenCalledTimes(1);
    });
});
