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

describe('globalInfoBarStore 同文折叠与延迟移除', () => {
    it('同 tone 同一句正文的不同来源只留一条，原位顶替成最新', () => {
        globalInfoBarStore.push({ key: 'other', tone: 'info', title: '别的' });
        globalInfoBarStore.push({ key: 'chat:a:history', tone: 'danger', title: '历史消息读取失败', content: '连接出错' });
        globalInfoBarStore.push({ key: 'chat:a:settings', tone: 'danger', title: '账号设置读取失败', content: '连接出错' });

        const bars = globalInfoBarStore.getSnapshot().bars;
        expect(bars).toHaveLength(2);
        expect(bars[1].id).toBe('key:chat:a:settings');
        expect(bars[1].title).toBe('账号设置读取失败');
        // 被顶替的旧 id 清理幂等空转；新 id 正常移除
        globalInfoBarStore.remove('key:chat:a:history');
        expect(globalInfoBarStore.getSnapshot().bars).toHaveLength(2);
        globalInfoBarStore.remove('key:chat:a:settings');
        expect(globalInfoBarStore.getSnapshot().bars).toHaveLength(1);
    });

    it('没有正文或正文不同的 bar 不折叠', () => {
        globalInfoBarStore.push({ key: 'a', tone: 'danger', title: 'A' });
        globalInfoBarStore.push({ key: 'b', tone: 'danger', title: 'B' });
        globalInfoBarStore.push({ key: 'c', tone: 'danger', title: 'C', content: '原因一' });
        globalInfoBarStore.push({ key: 'd', tone: 'danger', title: 'D', content: '原因二' });
        expect(globalInfoBarStore.getSnapshot().bars).toHaveLength(4);
    });

    it('removeSoon 延迟窗口内同 key 再 push 取消移除', () => {
        vi.useFakeTimers();
        try {
            globalInfoBarStore.push({ key: 'a', tone: 'danger', title: 'T', content: 'X' });
            globalInfoBarStore.removeSoon('key:a', 1000);
            globalInfoBarStore.push({ key: 'a', tone: 'danger', title: 'T', content: 'X' });
            vi.advanceTimersByTime(1500);
            expect(globalInfoBarStore.getSnapshot().bars).toHaveLength(1);
        } finally {
            vi.useRealTimers();
        }
    });

    it('removeSoon 到期移除', () => {
        vi.useFakeTimers();
        try {
            globalInfoBarStore.push({ key: 'a', tone: 'danger', title: 'T', content: 'X' });
            globalInfoBarStore.removeSoon('key:a', 1000);
            vi.advanceTimersByTime(1000);
            expect(globalInfoBarStore.getSnapshot().bars).toHaveLength(0);
        } finally {
            vi.useRealTimers();
        }
    });
});
