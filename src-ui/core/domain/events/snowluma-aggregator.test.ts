import { describe, expect, it } from 'vitest';
import { initialSnowlumaState, reduceSnowluma } from './snowluma-aggregator';

describe('reduceSnowluma', () => {
    it('不同主机的 daemon 状态互不覆盖', () => {
        let state = reduceSnowluma(initialSnowlumaState, {
            kind: 'snowluma_daemon_state_changed',
            state: 'ready',
            ref_count: 1,
            server_id: 'local',
        });
        state = reduceSnowluma(state, {
            kind: 'snowluma_daemon_state_changed',
            state: 'crashed',
            ref_count: 0,
            server_id: 'server-a',
        });

        expect(state.daemonStates.local).toBe('ready');
        expect(state.daemonStates['server-a']).toBe('crashed');
    });

    it('探测不可用只把对应 Bot 登录态降为未知', () => {
        let state = reduceSnowluma(initialSnowlumaState, {
            kind: 'snowluma_login_state_changed',
            bot_id: '10001',
            state: 'logged_in',
        });
        state = reduceSnowluma(state, {
            kind: 'snowluma_login_state_changed',
            bot_id: '10002',
            state: 'logged_in',
        });

        state = reduceSnowluma(state, {
            kind: 'snowluma_login_probe_unavailable',
            bot_id: '10001',
        });

        expect(state.byBot['10001']?.loginState).toBeNull();
        expect(state.byBot['10002']?.loginState).toBe('logged_in');
    });

    it('Bot 停止后清掉上个进程的登录状态', () => {
        let state = reduceSnowluma(initialSnowlumaState, {
            kind: 'snowluma_login_state_changed',
            bot_id: '10001',
            state: 'logged_in',
        });

        state = reduceSnowluma(state, {
            kind: 'bot_state_changed',
            snapshot: {
                bot_id: '10001',
                state: 'stopped',
                revision: 2,
                token_generation: 1,
                pending_restart: false,
            },
        });

        expect(state.byBot['10001']).toBeUndefined();
    });

    it('进程退出时清掉上个进程的登录状态', () => {
        let state = reduceSnowluma(initialSnowlumaState, {
            kind: 'snowluma_login_state_changed',
            bot_id: '10001',
            state: 'logged_in',
        });

        state = reduceSnowluma(state, {
            kind: 'bot_process_exited',
            bot_id: '10001',
        });

        expect(state.byBot['10001']).toBeUndefined();
    });
});
