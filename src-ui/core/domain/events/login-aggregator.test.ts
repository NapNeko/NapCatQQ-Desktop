import { describe, expect, it } from 'vitest';
import {
    initialNapcatLoginState,
    reduceNapcatLogin,
} from './login-aggregator';

describe('reduceNapcatLogin', () => {
    it('新 WebUI binding 不继承上个进程的在线状态', () => {
        let state = reduceNapcatLogin(initialNapcatLoginState, {
            kind: 'napcat_webui_available',
            bot_id: '10001',
            port: 6099,
            token: 'old-token',
        });
        state = reduceNapcatLogin(state, {
            kind: 'napcat_login_online',
            bot_id: '10001',
            online: true,
        });

        state = reduceNapcatLogin(state, {
            kind: 'napcat_webui_available',
            bot_id: '10001',
            port: 6100,
            token: 'new-token',
        });

        expect(state.byBot['10001']?.webui).toEqual({
            port: 6100,
            token: 'new-token',
        });
        expect(state.byBot['10001']?.online).toBeNull();
    });

    it('连续探测失败把旧在线状态降为未知', () => {
        let state = reduceNapcatLogin(initialNapcatLoginState, {
            kind: 'napcat_login_online',
            bot_id: '10001',
            online: true,
        });

        state = reduceNapcatLogin(state, {
            kind: 'napcat_login_probe_unavailable',
            bot_id: '10001',
        });

        expect(state.byBot['10001']?.online).toBeNull();
    });

    it('进程退出时即使 binding 已清也会清掉旧在线状态', () => {
        let state = reduceNapcatLogin(initialNapcatLoginState, {
            kind: 'napcat_login_online',
            bot_id: '10001',
            online: true,
        });

        state = reduceNapcatLogin(state, {
            kind: 'bot_process_exited',
            bot_id: '10001',
        });

        expect(state.byBot['10001']?.online).toBeNull();
    });
});
