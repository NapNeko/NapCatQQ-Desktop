import { describe, expect, it } from 'vitest';

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import {
    getRuntimeRequirement,
    runtimeReadinessNotice,
    runtimeStartBlockReason,
} from './runtime-gate';

function botConfig(overrides: Partial<BotConfig['bot']> = {}): BotConfig {
    return {
        bot: {
            name: 'bot',
            QQID: 10001,
            musicSignUrl: '',
            autoRestartSchedule: { enable: false, time_unit: 'd', duration: 1 },
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

describe('runtime gate matrix', () => {
    it('keeps local native as local direct', () => {
        const config = botConfig();

        expect(getRuntimeRequirement(config)).toEqual({
            kind: 'local-direct',
            backend: 'napcat',
        });
    });

    it('keeps remote docker as remote docker', () => {
        const config = botConfig({
            runtime_target: 'server-a',
            deploymentType: 'docker',
            backend_type: 'snowluma',
        });

        expect(getRuntimeRequirement(config)).toEqual({
            kind: 'remote-docker',
            hostId: 'remote:server-a',
            backend: 'snowluma',
        });
    });

    it('blocks local docker before runtime checks', () => {
        const config = botConfig({ deploymentType: 'docker' });

        expect(getRuntimeRequirement(config)).toEqual({
            kind: 'unsupported-local-docker',
            backend: 'napcat',
        });
        expect(runtimeStartBlockReason({ config })).toContain('本机不支持 Docker 部署');
        expect(runtimeReadinessNotice({ config })).toEqual({
            tone: 'warn',
            text: '本机不支持 Docker 部署，请改为直接运行或选择远程主机',
        });
    });
});
