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
            snowlumaDockerEndpointsReady: true,
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
            snowlumaDockerEndpointsReady: true,
        });
        expect(s.session?.label).toBe('探测登录');
    });

    it('SL running 探测不可用时显示 WebUI 不可用', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'snowluma',
            pendingRestart: false,
            needsQrLogin: false,
            snowlumaLoginState: null,
            snowlumaDaemonState: 'ready',
            snowlumaProbeUnavailable: true,
            snowlumaDockerEndpointsReady: true,
        });
        expect(s.session?.label).toBe('WebUI 不可用');
    });

    it('SL running 隧道未就绪时显示等待 WebUI', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'snowluma',
            pendingRestart: false,
            needsQrLogin: false,
            snowlumaLoginState: null,
            snowlumaDaemonState: 'ready',
            snowlumaDockerEndpointsReady: false,
        });
        expect(s.session?.label).toBe('等待 WebUI');
    });

    it('SL 已停止时不受共享 Daemon 状态影响', () => {
        const s = buildBotListCardStatus({
            state: 'stopped',
            flavor: 'snowluma',
            pendingRestart: false,
            needsQrLogin: false,
            snowlumaLoginState: 'logged_in',
            snowlumaDaemonState: 'crashed',
            snowlumaDockerEndpointsReady: true,
        });
        expect(s.lifecycle.label).toBe('已停止');
        expect(s.session).toBeNull();
    });

    it('NC running + token_rejected 显示 WebUI 鉴权失败', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'napcat',
            pendingRestart: false,
            needsQrLogin: false,
            isOnline: null,
            napcatAuthIssue: 'token_rejected',
        });
        expect(s.session?.label).toBe('WebUI 鉴权失败');
        expect(s.session?.tone).toBe('danger');
    });

    it('NC running + rate_limited 显示 WebUI 登录受限', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'napcat',
            pendingRestart: false,
            needsQrLogin: false,
            isOnline: null,
            napcatAuthIssue: 'rate_limited',
        });
        expect(s.session?.label).toBe('WebUI 登录受限');
    });

    it('NC running + unreachable 显示 WebUI 不可用', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'napcat',
            pendingRestart: false,
            needsQrLogin: false,
            isOnline: null,
            napcatAuthIssue: 'unreachable',
        });
        expect(s.session?.label).toBe('WebUI 不可用');
    });

    it('NC 待扫码优先于鉴权问题', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'napcat',
            pendingRestart: false,
            needsQrLogin: true,
            isOnline: null,
            napcatAuthIssue: 'token_rejected',
        });
        expect(s.session?.label).toBe('待扫码');
    });

    it('NC online=true 优先于鉴权问题', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'napcat',
            pendingRestart: false,
            needsQrLogin: false,
            isOnline: true,
            napcatAuthIssue: 'token_rejected',
        });
        expect(s.session?.label).toBe('QQ 已登录');
    });

    it('NC 无鉴权问题仍显示探测登录', () => {
        const s = buildBotListCardStatus({
            state: 'running',
            flavor: 'napcat',
            pendingRestart: false,
            needsQrLogin: false,
            isOnline: null,
            napcatAuthIssue: null,
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
