// Bot 状态展示规范单测（进程 + 会话词汇表）。

import { describe, expect, it } from 'vitest';
import { buildBotListCardStatus } from './bot-status-presentation';

describe('buildBotListCardStatus', () => {
    it('NC running + online 显示统一 QQ 已登录', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'napcat',
            pendingRestart: false,
            needsQrLogin: false,
            isOnline: true,
        });
        expect(s.lifecycle.label).toBe('运行中');
        expect(s.session?.label).toBe('QQ 已登录');
        expect(s.alert).toBeNull();
    });

    it('NC running + QR 优先显示待扫码', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'napcat',
            pendingRestart: false,
            needsQrLogin: true,
            isOnline: null,
        });
        expect(s.session?.label).toBe('待扫码');
    });

    it('SL running + logged_in 与 NC 同文案', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'snowluma',
            pendingRestart: false,
            needsQrLogin: false,
            snowlumaLoginState: 'logged_in',
            snowlumaDaemonState: 'ready',
        });
        expect(s.session?.label).toBe('QQ 已登录');
        expect(s.session?.dot).toBe(true);
    });

    it('SL running 无 login 事件时显示探测登录', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'snowluma',
            pendingRestart: false,
            needsQrLogin: false,
            snowlumaLoginState: null,
            snowlumaDaemonState: 'ready',
        });
        expect(s.session?.label).toBe('探测登录');
    });

    it('待重启独立第三枚徽章', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'napcat',
            pendingRestart: true,
            needsQrLogin: false,
            isOnline: true,
        });
        expect(s.alert?.label).toBe('待重启');
        expect(s.lifecycle.label).toBe('运行中');
        expect(s.session?.label).toBe('QQ 已登录');
    });
});
