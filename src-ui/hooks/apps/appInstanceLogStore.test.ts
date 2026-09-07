import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const subscribeMock = vi.fn();

vi.mock('../../core/services/event-stream.service', () => ({
    eventStreamService: {
        subscribe: (...args: unknown[]) => subscribeMock(...args),
    },
}));

import { _resetDomainEventHubForTests } from '../../core/services/domain-event-hub';
import {
    appInstanceLogStore,
    clearAppInstanceLogs,
    dropAppInstanceLogs,
} from './appInstanceLogStore';
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
    appInstanceLogStore._reset();
});

afterEach(() => {
    appInstanceLogStore._reset();
    _resetDomainEventHubForTests();
});

describe('appInstanceLogStore', () => {
    it('React 订阅者卸载后仍在攒行（离开日志页不丢）', async () => {
        const sub = installSubscribeMock();
        const unsub = appInstanceLogStore.subscribe(() => {});
        await flush();
        unsub();

        const handler = sub.getHandler();
        expect(handler).toBeTruthy();
        handler!({
            kind: 'app_instance_log_appended',
            instance_id: 'inst-a',
            line: '[Karin][INFO] heartbeat #1',
        });
        handler!({
            kind: 'app_instance_log_appended',
            instance_id: 'inst-a',
            line: '[Karin][INFO] heartbeat #2',
        });

        const logs = appInstanceLogStore.getSnapshot().byId['inst-a'] ?? [];
        expect(logs).toHaveLength(2);
        expect(logs[1].text).toContain('heartbeat #2');
    });

    it('清空只清当前实例', async () => {
        const sub = installSubscribeMock();
        const unsub = appInstanceLogStore.subscribe(() => {});
        await flush();

        sub.getHandler()!({
            kind: 'app_instance_log_appended',
            instance_id: 'a',
            line: 'a1',
        });
        sub.getHandler()!({
            kind: 'app_instance_log_appended',
            instance_id: 'b',
            line: 'b1',
        });
        clearAppInstanceLogs('a');

        expect(appInstanceLogStore.getSnapshot().byId['a']).toEqual([]);
        expect(appInstanceLogStore.getSnapshot().byId['b']).toHaveLength(1);
        unsub();
    });

    it('删除实例丢掉缓冲', async () => {
        const sub = installSubscribeMock();
        const unsub = appInstanceLogStore.subscribe(() => {});
        await flush();

        sub.getHandler()!({
            kind: 'app_instance_log_appended',
            instance_id: 'gone',
            line: 'x',
        });
        dropAppInstanceLogs('gone');
        expect(appInstanceLogStore.getSnapshot().byId['gone']).toBeUndefined();
        unsub();
    });
});
