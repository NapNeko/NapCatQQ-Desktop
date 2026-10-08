import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';

const subscribeMock = vi.fn();
const tailLogMock = vi.fn();
const getConfigMock = vi.fn();

vi.mock('../../core/services/event-stream.service', () => ({
    eventStreamService: {
        subscribe: (...args: unknown[]) => subscribeMock(...args),
    },
}));

vi.mock('../../core/services/bot.service', () => ({
    botService: {
        tailLog: (...args: unknown[]) => tailLogMock(...args),
        getConfig: (...args: unknown[]) => getConfigMock(...args),
    },
}));

import { _resetDomainEventHubForTests } from '../../core/services/domain-event-hub';
import { botLogStore, hydrateBotLogs } from './botLogStore';
import { useBotLogStream } from './useBotLogStream';
import type { BotActorSnapshot, DomainEvent, LogSnapshot } from '../../core/ipc/types';

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

function snapshotFor(botId: string, state: BotActorSnapshot['state']): BotActorSnapshot {
    return {
        bot_id: botId,
        state,
        revision: 1,
        token_generation: 0,
        pending_restart: false,
    };
}

beforeEach(() => {
    subscribeMock.mockReset();
    tailLogMock.mockReset();
    getConfigMock.mockReset();
    // hydrate 的两个拉取默认「盘上没历史、配置认不出后端」，单个用例再 Once 覆盖
    tailLogMock.mockResolvedValue({ lines: [], total_lines: 0 });
    getConfigMock.mockResolvedValue(null);
    _resetDomainEventHubForTests();
    botLogStore._reset();
});

afterEach(() => {
    botLogStore._reset();
    _resetDomainEventHubForTests();
});

describe('botLogStore', () => {
    it('日志页卸载期间推来的 bot_log_appended 不丢（旧实现此断言必红）', async () => {
        const sub = installSubscribeMock();

        const first = renderHook(() => useBotLogStream('bot-a'));
        await flush();

        act(() => {
            sub.getHandler()!({
                kind: 'bot_log_appended',
                bot_id: 'bot-a',
                line: '第一轮的第一行',
                channel: 'stdout',
            });
        });
        expect(first.result.current.logs.map((l) => l.text)).toContain('第一轮的第一行');

        // 切页：组件卸载。旧实现到这里断订，下面这行永久丢
        first.unmount();
        act(() => {
            sub.getHandler()!({
                kind: 'bot_log_appended',
                bot_id: 'bot-a',
                line: '切页期间的行',
                channel: 'stdout',
            });
        });

        const second = renderHook(() => useBotLogStream('bot-a'));
        await flush();
        expect(second.result.current.logs.map((l) => l.text)).toEqual([
            '第一轮的第一行',
            '切页期间的行',
        ]);
        second.unmount();
    });

    it('空历史快照不盖内存缓冲；有磁盘尾部才整块覆盖', async () => {
        const sub = installSubscribeMock();
        const unsub = botLogStore.subscribe(() => {});
        await flush();
        hydrateBotLogs('b');
        await flush();
        await flush();

        const handler = sub.getHandler()!;
        handler({ kind: 'bot_log_appended', bot_id: 'b', line: '攒下的增量', channel: 'stdout' });
        hydrateBotLogs('b');
        await flush();
        await flush();
        expect(botLogStore.getSnapshot().byId['b'].map((l) => l.text)).toEqual(['攒下的增量']);

        tailLogMock.mockResolvedValueOnce({
            lines: ['09-27 11:14:47 [info] nick | 盘上的尾巴'],
            total_lines: 1,
        });
        hydrateBotLogs('b');
        await flush();
        await flush();
        expect(tailLogMock).toHaveBeenLastCalledWith('b', 1000);
        expect(botLogStore.getSnapshot().byId['b'].map((l) => l.text)).toEqual([
            'nick | 盘上的尾巴',
        ]);
        unsub();
    });

    it('running 事件后新轮次清空，之后的行从头接', async () => {
        const sub = installSubscribeMock();
        const unsub = botLogStore.subscribe(() => {});
        await flush();
        const handler = sub.getHandler()!;

        handler({ kind: 'bot_log_appended', bot_id: 'r', line: '上一轮', channel: 'stdout' });
        handler({ kind: 'bot_log_appended', bot_id: 'other', line: '别的 bot', channel: 'stdout' });
        handler({ kind: 'bot_state_changed', snapshot: snapshotFor('r', 'stopped') });
        handler({ kind: 'bot_state_changed', snapshot: snapshotFor('r', 'running') });
        handler({ kind: 'bot_log_appended', bot_id: 'r', line: '这一轮', channel: 'stdout' });

        const byId = botLogStore.getSnapshot().byId;
        expect(byId['r'].map((l) => l.text)).toEqual(['这一轮']);
        expect(byId['other']).toHaveLength(1);
        unsub();
    });

    it('翻 running 后开页不再拿盘上尾巴盖新一轮', async () => {
        const sub = installSubscribeMock();
        const unsub = botLogStore.subscribe(() => {});
        await flush();
        const handler = sub.getHandler()!;

        handler({ kind: 'bot_state_changed', snapshot: snapshotFor('w', 'stopped') });
        handler({ kind: 'bot_state_changed', snapshot: snapshotFor('w', 'running') });
        handler({ kind: 'bot_log_appended', bot_id: 'w', line: '新一轮第一行', channel: 'stdout' });

        const tail: Promise<LogSnapshot> = Promise.resolve({
            lines: ['上一轮的旧行'],
            total_lines: 1,
        });
        tailLogMock.mockReturnValueOnce(tail);
        tailLogMock.mockClear();
        hydrateBotLogs('w');
        await flush();
        await flush();
        expect(tailLogMock).not.toHaveBeenCalled();
        expect(botLogStore.getSnapshot().byId['w'].map((l) => l.text)).toEqual(['新一轮第一行']);
        unsub();
    });

    it('进程退出清缓冲，重启翻 running 再开新轮', async () => {
        const sub = installSubscribeMock();
        const unsub = botLogStore.subscribe(() => {});
        await flush();
        const handler = sub.getHandler()!;

        handler({ kind: 'bot_log_appended', bot_id: 'p', line: '退出前', channel: 'stdout' });
        handler({ kind: 'bot_process_exited', bot_id: 'p' });
        expect(botLogStore.getSnapshot().byId['p']).toEqual([]);

        handler({
            kind: 'bot_log_appended',
            bot_id: 'p',
            line: '退出后又推来的',
            channel: 'stdout',
        });
        handler({ kind: 'bot_state_changed', snapshot: snapshotFor('p', 'running') });
        handler({ kind: 'bot_log_appended', bot_id: 'p', line: '重启后', channel: 'stdout' });
        expect(botLogStore.getSnapshot().byId['p'].map((l) => l.text)).toEqual(['重启后']);
        unsub();
    });

    it('SL bot：daemon 增量进缓冲，bot 通道里的 [NapCat] 转发行过滤', async () => {
        const sub = installSubscribeMock();
        const unsub = botLogStore.subscribe(() => {});
        await flush();
        getConfigMock.mockResolvedValueOnce({ bot: { backend_type: 'snowluma' } });
        hydrateBotLogs('sl');
        await flush();
        await flush();

        const handler = sub.getHandler()!;
        handler({ kind: 'snowluma_daemon_log', line: '[stderr] daemon 报错' });
        handler({
            kind: 'bot_log_appended',
            bot_id: 'sl',
            line: '[NapCat] 转发行',
            channel: 'stdout',
        });
        handler({ kind: 'bot_log_appended', bot_id: 'sl', line: 'SL 正常行', channel: 'stdout' });

        const logs = botLogStore.getSnapshot().byId['sl'];
        expect(logs.map((l) => l.text)).toEqual(['[stderr] daemon 报错', 'SL 正常行']);
        expect(logs[0].channel).toBe('stderr');
        unsub();
    });
});
