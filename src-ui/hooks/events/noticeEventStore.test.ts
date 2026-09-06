import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const subscribeMock = vi.fn();

vi.mock('../../core/services/event-stream.service', () => ({
    eventStreamService: {
        subscribe: (...args: unknown[]) => subscribeMock(...args),
    },
}));

import { _resetDomainEventHubForTests } from '../../core/services/domain-event-hub';
import { noticeEventStore } from './noticeEventStore';
import type { DomainEvent } from '../../core/ipc/types';

type StreamHandler = (event: DomainEvent) => void;

function installSubscribeMock(): { getHandler: () => StreamHandler | null } {
    let streamHandler: StreamHandler | null = null;
    subscribeMock.mockImplementation(async (cb: StreamHandler) => {
        streamHandler = cb;
        return () => {
            if (streamHandler === cb) streamHandler = null;
        };
    });
    return { getHandler: () => streamHandler };
}

async function flush(): Promise<void> {
    await Promise.resolve();
    await Promise.resolve();
}

beforeEach(() => {
    subscribeMock.mockReset();
    _resetDomainEventHubForTests();
    noticeEventStore._reset();
});

afterEach(() => {
    noticeEventStore._reset();
    _resetDomainEventHubForTests();
});

describe('noticeEventStore', () => {
    it('只收通知相关事件，日志行不进缓冲', async () => {
        const sub = installSubscribeMock();
        const unsub = noticeEventStore.subscribe(() => { });
        await flush();

        const handler = sub.getHandler();
        expect(handler).toBeTruthy();

        for (let i = 0; i < 200; i += 1) {
            handler!({ kind: 'bot_log_appended', bot_id: '10001', line: `l${i}` } as DomainEvent);
        }
        handler!({ kind: 'bot_process_exited', bot_id: '10001', exit_code: 1 } as DomainEvent);

        const { events } = noticeEventStore.getSnapshot();
        expect(events).toHaveLength(1);
        expect(events[0].payload.kind).toBe('bot_process_exited');
        expect(typeof events[0].at).toBe('number');

        unsub();
    });

    it('React 订阅者全部卸载后仍在攒事件（跨路由保留）', async () => {
        const sub = installSubscribeMock();
        const unsub = noticeEventStore.subscribe(() => { });
        await flush();
        unsub();

        sub.getHandler()!({
            kind: 'napcat_login_invalidated',
            bot_id: '10001',
            reason: 'kicked',
        } as DomainEvent);

        expect(noticeEventStore.getSnapshot().events).toHaveLength(1);

        // 再次挂订阅者能直接读到之前攒的
        const seen: number[] = [];
        const unsub2 = noticeEventStore.subscribe(() => {
            seen.push(noticeEventStore.getSnapshot().events.length);
        });
        expect(noticeEventStore.getSnapshot().events).toHaveLength(1);
        sub.getHandler()!({ kind: 'bot_error', bot_id: 'x', message: 'boom' } as DomainEvent);
        expect(seen).toEqual([2]);
        unsub2();
    });

    it('最新在前，超出上限时丢最旧的', async () => {
        const sub = installSubscribeMock();
        const unsub = noticeEventStore.subscribe(() => { });
        await flush();

        for (let i = 0; i < 60; i += 1) {
            sub.getHandler()!({ kind: 'bot_error', bot_id: `b${i}`, message: 'x' } as DomainEvent);
        }
        const { events } = noticeEventStore.getSnapshot();
        expect(events).toHaveLength(50);
        expect((events[0].payload as { bot_id: string }).bot_id).toBe('b59');
        expect((events[49].payload as { bot_id: string }).bot_id).toBe('b10');

        unsub();
    });
});
