import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const subscribeMock = vi.fn();
const tailLogMock = vi.fn();

vi.mock('../../core/services/event-stream.service', () => ({
    eventStreamService: {
        subscribe: (...args: unknown[]) => subscribeMock(...args),
    },
}));

vi.mock('../../core/services/app-framework.service', () => ({
    appFrameworkService: {
        tailLog: (...args: unknown[]) => tailLogMock(...args),
    },
}));

import { _resetDomainEventHubForTests } from '../../core/services/domain-event-hub';
import {
    appInstanceLogStore,
    clearAppInstanceLogs,
    dropAppInstanceLogs,
    hydrateAppInstanceLogs,
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
    tailLogMock.mockReset();
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

    it('另起一轮就清空，之后的行从头接', async () => {
        const sub = installSubscribeMock();
        const unsub = appInstanceLogStore.subscribe(() => {});
        await flush();
        const handler = sub.getHandler()!;

        handler({ kind: 'app_instance_log_appended', instance_id: 'm', line: '上一轮' });
        handler({ kind: 'app_instance_log_appended', instance_id: 'other', line: '别的实例' });
        handler({ kind: 'app_instance_log_reset', instance_id: 'm' });
        handler({ kind: 'app_instance_log_appended', instance_id: 'm', line: '这一轮' });

        const byId = appInstanceLogStore.getSnapshot().byId;
        expect(byId['m'].map((l) => l.text)).toEqual(['这一轮']);
        expect(byId['other']).toHaveLength(1);
        unsub();
    });

    it('拉历史途中另起了一轮：晚到的旧快照不盖掉新一轮', async () => {
        let resolveTail!: (v: { lines: string[]; total_lines: number }) => void;
        tailLogMock.mockReturnValueOnce(
            new Promise((resolve) => {
                resolveTail = resolve;
            }),
        );
        const sub = installSubscribeMock();
        const unsub = appInstanceLogStore.subscribe(() => {});
        await flush();

        hydrateAppInstanceLogs('m');
        sub.getHandler()!({ kind: 'app_instance_log_reset', instance_id: 'm' });
        sub.getHandler()!({ kind: 'app_instance_log_appended', instance_id: 'm', line: '新一轮第一行' });
        resolveTail({ lines: ['上一轮 1', '上一轮 2'], total_lines: 2 });
        await flush();
        await flush();

        expect(appInstanceLogStore.getSnapshot().byId['m'].map((l) => l.text)).toEqual(['新一轮第一行']);
        unsub();
    });

    it('这一轮从开头就看着：开页不再拿盘上的尾巴盖，用户清空过的也不翻回来', async () => {
        const sub = installSubscribeMock();
        const unsub = appInstanceLogStore.subscribe(() => {});
        await flush();
        const handler = sub.getHandler()!;

        handler({ kind: 'app_instance_log_reset', instance_id: 'm' });
        handler({ kind: 'app_instance_log_appended', instance_id: 'm', line: '第一行' });
        hydrateAppInstanceLogs('m');
        expect(tailLogMock).not.toHaveBeenCalled();
        expect(appInstanceLogStore.getSnapshot().byId['m'].map((l) => l.text)).toEqual(['第一行']);

        clearAppInstanceLogs('m');
        hydrateAppInstanceLogs('m');
        expect(tailLogMock).not.toHaveBeenCalled();
        expect(appInstanceLogStore.getSnapshot().byId['m']).toEqual([]);
        unsub();
    });

    it('桌面端打开前就在跑的实例（没见过它的开头）：开页从盘上补', async () => {
        tailLogMock.mockResolvedValueOnce({ lines: ['09-27 11:14:47 [INFO] a | 早先的'], total_lines: 1 });
        hydrateAppInstanceLogs('running-before');
        await flush();
        await flush();
        expect(tailLogMock).toHaveBeenCalledWith('running-before', 1000);
        expect(appInstanceLogStore.getSnapshot().byId['running-before'].map((l) => l.text)).toEqual(['a | 早先的']);
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
