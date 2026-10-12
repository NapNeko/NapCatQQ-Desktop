// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DomainEvent } from '../../core/ipc/types';
import type { botService } from '../../core/services/bot.service';

type SnowlumaUiSnapshotDto = Awaited<ReturnType<typeof botService.listSnowlumaUiSnapshot>>;
const handlers = new Set<(event: DomainEvent) => void>();
const snapshotMock = vi.fn<() => Promise<SnowlumaUiSnapshotDto>>();

vi.mock('../../core/services/domain-event-hub', () => ({
    subscribeDomainEvents: (handler: (event: DomainEvent) => void) => {
        handlers.add(handler);
        return () => handlers.delete(handler);
    },
}));

vi.mock('../../core/services/bot.service', () => ({
    botService: {
        listSnowlumaUiSnapshot: () => snapshotMock(),
    },
}));

import { botSessionBadge } from '../../core/domain/bot/bot-status-presentation';
import { snowlumaStore } from './snowlumaStore';

const BOT = '10001';
const OTHER_BOT = '10002';
let unsubscribe = () => {};

function snapshot(unavailable: boolean, botId = BOT): SnowlumaUiSnapshotDto {
    const row = {
        bot_id: botId,
        injected: true,
        uin: botId,
        login_state: unavailable ? null : ('logged_in' as const),
        endpoints_ready: true,
        probe_unavailable: unavailable,
    };
    return {
        daemon_state: null,
        daemon_states: { remote: 'ready' },
        bots: [row],
    };
}

function emit(event: DomainEvent): void {
    for (const handler of handlers) handler(event);
}

function emitRunning(botId = BOT, state: 'starting' | 'running' | 'stopped' = 'running'): void {
    emit({
        kind: 'bot_state_changed',
        snapshot: {
            bot_id: botId,
            state,
            revision: 1,
            token_generation: 1,
            pending_restart: false,
        },
        reason: 'bootstrap_reconcile',
    });
}

async function flushHydrate(): Promise<void> {
    for (let turn = 0; turn < 8; turn += 1) await Promise.resolve();
}

function sessionLabel(): string | undefined {
    const bot = snowlumaStore.getSnapshot().byBot[BOT];
    return botSessionBadge({
        flavor: 'snowluma',
        state: 'running',
        needsQrLogin: false,
        snowlumaDaemonState: 'ready',
        snowlumaLoginState: bot?.loginState,
        snowlumaProbeUnavailable: bot?.probeUnavailable,
        snowlumaDockerEndpointsReady: bot?.dockerEndpointsReady,
    })?.label;
}

beforeEach(() => {
    snowlumaStore._reset();
    handlers.clear();
    snapshotMock.mockReset();
});

afterEach(() => {
    unsubscribe();
    snowlumaStore._reset();
    handlers.clear();
    vi.useRealTimers();
});

describe('SnowLuma 登录探测快照恢复', () => {
    it('页面晚于探测失败订阅时仍显示 WebUI 不可用', async () => {
        snapshotMock.mockResolvedValue(snapshot(true));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();

        expect(snowlumaStore.getSnapshot().byBot[BOT].probeUnavailable).toBe(true);
        expect(sessionLabel()).toBe('WebUI 不可用');
    });

    it('重连接管的 hydrate 不把探测失败盖回探测登录', async () => {
        snapshotMock.mockResolvedValueOnce(snapshot(false));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();
        expect(sessionLabel()).toBe('QQ 已登录');

        emit({ kind: 'snowluma_login_probe_unavailable', bot_id: BOT });
        snapshotMock.mockResolvedValueOnce(snapshot(true));
        emitRunning();
        await flushHydrate();

        expect(snapshotMock).toHaveBeenCalledTimes(2);
        expect(sessionLabel()).toBe('WebUI 不可用');
    });

    it('登录探测恢复后的快照清除失败标记', async () => {
        snapshotMock.mockResolvedValueOnce(snapshot(true));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();

        snapshotMock.mockResolvedValueOnce(snapshot(false));
        emitRunning();
        await flushHydrate();

        expect(snowlumaStore.getSnapshot().byBot[BOT].probeUnavailable).toBe(false);
        expect(sessionLabel()).toBe('QQ 已登录');
    });

    it('迟到的失败快照不能覆盖已恢复的登录事件', async () => {
        snapshotMock.mockResolvedValueOnce(snapshot(true));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();

        let resolveSnapshot!: (value: SnowlumaUiSnapshotDto) => void;
        snapshotMock.mockReturnValueOnce(
            new Promise((resolve) => {
                resolveSnapshot = resolve;
            }),
        );
        emitRunning();
        emit({ kind: 'snowluma_login_state_changed', bot_id: BOT, state: 'logged_in' });
        resolveSnapshot(snapshot(true));
        await flushHydrate();

        expect(snowlumaStore.getSnapshot().byBot[BOT].probeUnavailable).toBe(false);
        expect(sessionLabel()).toBe('QQ 已登录');
    });

    it('hydrate 等待期间的日志事件不丢弃登录快照', async () => {
        let resolveSnapshot!: (value: SnowlumaUiSnapshotDto) => void;
        snapshotMock.mockReturnValueOnce(
            new Promise((resolve) => {
                resolveSnapshot = resolve;
            }),
        );
        unsubscribe = snowlumaStore.subscribe(() => {});

        emit({ kind: 'bot_log_appended', bot_id: BOT, line: 'Bot 正在处理消息' });
        emit({ kind: 'snowluma_daemon_log', line: 'OneBot 已连接' });
        resolveSnapshot(snapshot(false));
        await flushHydrate();

        expect(snowlumaStore.getSnapshot().byBot[BOT].loginState).toBe('logged_in');
        expect(sessionLabel()).toBe('QQ 已登录');
    });
});

describe('SnowLuma 多 Bot 快照隔离', () => {
    it('单个 Bot 启动只更新目标行，不重置其他已登录账号或共享 Ready', async () => {
        const initial = snapshot(false);
        initial.bots.push(...snapshot(false, OTHER_BOT).bots);
        snapshotMock.mockResolvedValueOnce(initial);
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();
        const original = snowlumaStore.getSnapshot().byBot[BOT];

        const next = snapshot(true);
        next.bots.push({ ...snapshot(false, OTHER_BOT).bots[0], login_state: 'starting' });
        next.daemon_states.remote = 'starting';
        snapshotMock.mockResolvedValueOnce(next);
        emitRunning(OTHER_BOT, 'starting');
        await flushHydrate();

        expect(snowlumaStore.getSnapshot().byBot[BOT]).toBe(original);
        expect(snowlumaStore.getSnapshot().byBot[OTHER_BOT].loginState).toBe('starting');
        expect(snowlumaStore.getSnapshot().daemonStates.remote).toBe('ready');
    });

    it.each(['missing', 'probe_failed'] as const)(
        '全量补齐快照 %s 不清掉事件已确认的登录',
        async (mode) => {
            vi.useFakeTimers();
            snapshotMock.mockResolvedValueOnce({ daemon_state: null, daemon_states: {}, bots: [] });
            unsubscribe = snowlumaStore.subscribe(() => {});
            await flushHydrate();
            emit({ kind: 'snowluma_login_state_changed', bot_id: BOT, state: 'logged_in' });
            const next = snapshot(true);
            if (mode === 'missing') next.bots = [];
            snapshotMock.mockResolvedValueOnce(next);
            await vi.advanceTimersByTimeAsync(250);

            expect(snowlumaStore.getSnapshot().byBot[BOT].loginState).toBe('logged_in');
            expect(snowlumaStore.getSnapshot().byBot[BOT].probeUnavailable).toBe(
                mode === 'probe_failed',
            );
        },
    );

    it('目标快照探测失败保留已确认账号，同时更新探测不可用告警', async () => {
        snapshotMock.mockResolvedValueOnce(snapshot(false));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();
        snapshotMock.mockResolvedValueOnce(snapshot(true));
        emitRunning();
        await flushHydrate();

        expect(snowlumaStore.getSnapshot().byBot[BOT]).toMatchObject({
            loginState: 'logged_in',
            uin: BOT,
            injected: true,
            dockerEndpointsReady: true,
            probeUnavailable: true,
        });
    });

    it('停止事件即时删除登录状态，迟到快照不得恢复它', async () => {
        snapshotMock.mockResolvedValueOnce(snapshot(false));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();
        let resolveSnapshot!: (value: SnowlumaUiSnapshotDto) => void;
        snapshotMock.mockReturnValueOnce(
            new Promise((resolve) => {
                resolveSnapshot = resolve;
            }),
        );
        emitRunning();
        emitRunning(BOT, 'stopped');
        expect(snowlumaStore.getSnapshot().byBot[BOT]).toBeUndefined();
        resolveSnapshot(snapshot(false));
        await flushHydrate();
        expect(snowlumaStore.getSnapshot().byBot[BOT]).toBeUndefined();
    });

    it('显式掉线事件仍即时替换已确认的登录', async () => {
        snapshotMock.mockResolvedValueOnce(snapshot(false));
        unsubscribe = snowlumaStore.subscribe(() => {});
        await flushHydrate();
        emit({ kind: 'snowluma_login_state_changed', bot_id: BOT, state: 'disconnected' });
        expect(snowlumaStore.getSnapshot().byBot[BOT].loginState).toBe('disconnected');
    });

    it('全量请求执行期间的新 Bot 启动会排队补齐自己的新快照', async () => {
        let resolveFirst!: (value: SnowlumaUiSnapshotDto) => void;
        snapshotMock
            .mockReturnValueOnce(
                new Promise((resolve) => {
                    resolveFirst = resolve;
                }),
            )
            .mockResolvedValueOnce(snapshot(false, OTHER_BOT));
        unsubscribe = snowlumaStore.subscribe(() => {});
        emitRunning(OTHER_BOT);
        resolveFirst(snapshot(false));
        await flushHydrate();

        expect(snapshotMock).toHaveBeenCalledTimes(2);
        expect(snowlumaStore.getSnapshot().byBot[BOT].loginState).toBe('logged_in');
        expect(snowlumaStore.getSnapshot().byBot[OTHER_BOT].loginState).toBe('logged_in');
    });
});
