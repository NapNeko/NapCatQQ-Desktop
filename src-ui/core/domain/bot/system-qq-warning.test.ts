import { describe, expect, it } from 'vitest';

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import { isSystemQqWarningDismissed, launchesLocalQq } from './system-qq-warning';

function botConfig(overrides: Partial<BotConfig['bot']> = {}): BotConfig {
    return {
        bot: {
            name: 'bot',
            QQID: 10001,
            musicSignUrl: '',
            autoRestartSchedule: {
                enable: false,
                mode: 'interval',
                time_unit: 'd',
                duration: 1,
                cron: '',
            },
            offlineAutoRestart: false,
            runtime_target: 'local',
            backend_type: 'napcat',
            deploymentType: 'native',
            ...overrides,
        },
        connect: {
            httpServers: [],
            httpSseServers: [],
            httpClients: [],
            websocketServers: [],
            websocketClients: [],
            plugins: [],
        },
        advanced: {
            autoStart: false,
            offlineNotice: false,
            parseMultMsg: false,
            packetServer: '',
            packetBackend: '',
            enableLocalFile2Url: false,
            fileLog: true,
            consoleLog: true,
            fileLogLevel: 'info',
            consoleLogLevel: 'info',
            o3HookMode: 0,
            bypass: {
                hook: false,
                window: false,
                module: false,
                process: false,
                container: false,
                js: false,
            },
        },
    };
}

describe('launchesLocalQq', () => {
    it('本机 NapCat 直接运行要用本机 QQ', () => {
        expect(launchesLocalQq(botConfig())).toBe(true);
    });

    it('SnowLuma 冷启动要用本机 QQ', () => {
        expect(launchesLocalQq(botConfig({ backend_type: 'snowluma' }))).toBe(true);
        expect(
            launchesLocalQq(
                botConfig({ backend_type: 'snowluma', snowlumaStartMode: { mode: 'cold_start' } }),
            ),
        ).toBe(true);
    });

    it('SnowLuma 热启动接的是用户自己开着的 QQ，不问', () => {
        expect(
            launchesLocalQq(
                botConfig({ backend_type: 'snowluma', snowlumaStartMode: { mode: 'hot_start' } }),
            ),
        ).toBe(false);
    });

    it('远端与本机 Docker 不由桌面端拉起本机 QQ', () => {
        expect(launchesLocalQq(botConfig({ runtime_target: 'server-a' }))).toBe(false);
        expect(launchesLocalQq(botConfig({ deploymentType: 'docker' }))).toBe(false);
    });
});

describe('isSystemQqWarningDismissed', () => {
    it('默认要提醒，勾过之后不再提醒', () => {
        localStorage.clear();
        expect(isSystemQqWarningDismissed()).toBe(false);
        localStorage.setItem('ncd.bot.systemQqWarning.dismissed.v1', '1');
        expect(isSystemQqWarningDismissed()).toBe(true);
    });
});
