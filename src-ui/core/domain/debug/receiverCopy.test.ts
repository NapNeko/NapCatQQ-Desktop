import { describe, expect, it } from 'vitest';
import type { DebugReceiverInfo } from '../../ipc/generated/debug/DebugReceiverInfo';
import type { DebugReceiverState } from '../../ipc/generated/debug/DebugReceiverState';
import { activeReceivers, receiverStateCopy } from './receiverCopy';

describe('receiverStateCopy', () => {
    it('四种状态', () => {
        expect(receiverStateCopy({ state: 'connecting' })).toEqual({ text: '连接中', tone: 'neutral' });
        expect(receiverStateCopy({ state: 'connected' })).toEqual({ text: '已连接', tone: 'success' });
        expect(receiverStateCopy({ state: 'reconnecting', attempt: 3, retry_in_ms: 3500 })).toEqual({
            text: '重连中（第 3 次，4 秒后）',
            tone: 'warning',
        });
        expect(receiverStateCopy({ state: 'reconnecting', attempt: 1, retry_in_ms: 0 }).text).toBe('重连中（第 1 次，1 秒后）');
        expect(receiverStateCopy({ state: 'stopped', reason: 'Bot 已停止' }).text).toBe('已停止：Bot 已停止');
        expect(receiverStateCopy({ state: 'stopped', reason: '' }).text).toBe('已停止');
    });
});

describe('activeReceivers', () => {
    const r = (bot_id: string, state: DebugReceiverState): DebugReceiverInfo => ({
        bot_id,
        source: { kind: 'internal' },
        state,
        buffered: 0,
        dropped_total: 0,
        first_seq: 1,
        viewers: 0,
    });

    it('去掉已停止的', () => {
        const list = [r('a', { state: 'connected' }), r('b', { state: 'stopped', reason: 'x' }), r('c', { state: 'connecting' })];
        expect(activeReceivers(list).map((x) => x.bot_id)).toEqual(['a', 'c']);
        expect(activeReceivers(undefined)).toEqual([]);
    });
});
