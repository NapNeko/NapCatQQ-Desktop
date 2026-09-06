import { describe, expect, it } from 'vitest';
import { buildNotices, isNoticeEvent } from './notice-aggregator';
import type { DomainEvent } from '../../ipc/types';

describe('isNoticeEvent', () => {
    it('日志 / 指标 / 进度类高频事件不进通知缓冲', () => {
        const noisy: DomainEvent[] = [
            { kind: 'bot_log_appended', bot_id: '10001', line: 'hello' } as DomainEvent,
            { kind: 'desktop_log_appended', line: 'x' } as DomainEvent,
            { kind: 'snowluma_daemon_log', line: 'x' } as DomainEvent,
            {
                kind: 'task_progress',
                task_id: 't',
                progress: 10,
                message: '',
            } as DomainEvent,
        ];
        for (const e of noisy) expect(isNoticeEvent(e)).toBe(false);
    });

    it('正常退出与 daemon 非崩溃态变化也不算通知', () => {
        expect(
            isNoticeEvent({
                kind: 'bot_process_exited',
                bot_id: '10001',
                exit_code: 0,
            } as DomainEvent),
        ).toBe(false);
        expect(
            isNoticeEvent({
                kind: 'snowluma_daemon_state_changed',
                state: 'ready',
                ref_count: 1,
            } as DomainEvent),
        ).toBe(false);
    });

    it('崩溃 / 掉线 / 等待扫码算通知', () => {
        expect(
            isNoticeEvent({
                kind: 'bot_process_exited',
                bot_id: '10001',
                exit_code: 1,
            } as DomainEvent),
        ).toBe(true);
        expect(
            isNoticeEvent({
                kind: 'napcat_login_invalidated',
                bot_id: '10001',
                reason: 'kicked',
            } as DomainEvent),
        ).toBe(true);
        expect(
            isNoticeEvent({
                kind: 'napcat_login_qrcode',
                bot_id: '10001',
            } as DomainEvent),
        ).toBe(true);
    });
});

describe('buildNotices 运行时通知', () => {
    it('带上事件收到时刻（转成 Unix 秒），同 bot 同 kind 只留最新一条', () => {
        const notices = buildNotices({
            bootstrap: null,
            releases: null,
            recentEvents: [
                {
                    at: 1_700_000_120_500,
                    payload: {
                        kind: 'bot_process_exited',
                        bot_id: '10001',
                        exit_code: 1,
                    } as DomainEvent,
                },
                {
                    at: 1_700_000_000_000,
                    payload: {
                        kind: 'bot_process_exited',
                        bot_id: '10001',
                        exit_code: 137,
                    } as DomainEvent,
                },
            ],
        });

        expect(notices).toHaveLength(1);
        expect(notices[0].id).toBe('runtime:bot_process_exited:10001');
        expect(notices[0].timestamp).toBe(1_700_000_120);
        expect(notices[0].detail).toBe('退出码 1');
    });

    it('没有 at 的事件不带 timestamp', () => {
        const notices = buildNotices({
            bootstrap: null,
            releases: null,
            recentEvents: [
                {
                    payload: {
                        kind: 'napcat_login_qrcode',
                        bot_id: '10001',
                    } as DomainEvent,
                },
            ],
        });
        expect(notices[0].timestamp).toBeUndefined();
    });

    it('danger 排在 info 前面', () => {
        const notices = buildNotices({
            bootstrap: null,
            releases: null,
            recentEvents: [
                {
                    at: 2_000,
                    payload: {
                        kind: 'napcat_login_qrcode',
                        bot_id: 'a',
                    } as DomainEvent,
                },
                {
                    at: 1_000,
                    payload: {
                        kind: 'bot_error',
                        bot_id: 'b',
                        message: 'boom',
                    } as DomainEvent,
                },
            ],
        });
        expect(notices.map((n) => n.tone)).toEqual(['danger', 'info']);
    });
});
