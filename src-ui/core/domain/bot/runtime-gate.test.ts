import { describe, expect, it } from 'vitest';

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { ComponentId } from '../../ipc/generated/domain/ComponentId';
import type { DependencyNode } from '../../ipc/generated/domain/DependencyNode';
import type { RuntimeReadiness } from '../../ipc/generated/domain/RuntimeReadiness';
import {
    getRuntimeRequirement,
    runtimeReadinessNotice,
    runtimeSaveBlockReason,
    runtimeStartBlockReason,
} from './runtime-gate';

function node(id: ComponentId, status: DependencyNode['status']): DependencyNode {
    return { target: { kind: 'component', id }, required_by: [], version_reqs: [], status };
}

const ok: DependencyNode['status'] = { state: 'satisfied', version: null, source: null };

function readiness(
    root: ComponentId,
    rootStatus: DependencyNode['status'],
    deps: DependencyNode[],
    hostId = 'local',
): RuntimeReadiness {
    return {
        root: node(root, rootStatus),
        plan: { root, host_id: hostId, phase: 'run', nodes: deps },
    };
}

const NAMES = { napcat: 'NapCat', snowluma: 'SnowLuma', qq: 'QQ', nodejs: 'Node.js' };

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

    it('transport failure wins over component state on remote direct', () => {
        const config = botConfig({
            runtime_target: 'server-a',
            backend_type: 'snowluma',
            deploymentType: 'native',
        });
        expect(
            runtimeStartBlockReason({
                config,
                remoteTransport: { reachable: false, label: 'kunming' },
                remoteDirect: {
                    readiness: readiness('snowluma', { state: 'missing' }, [], 'remote:server-a'),
                    probing: false,
                },
            }),
        ).toBe('远端主机 kunming 连接中断');
    });

    it('remote direct ready gives no block and an ok notice', () => {
        const config = botConfig({
            runtime_target: 'server-a',
            backend_type: 'snowluma',
            deploymentType: 'native',
        });
        const args = {
            config,
            remoteTransport: { reachable: true, label: 'kunming' },
            remoteDirect: {
                readiness: readiness('snowluma', ok, [node('qq', ok)], 'remote:server-a'),
                probing: false,
            },
        };
        expect(runtimeStartBlockReason(args)).toBeNull();
        expect(runtimeSaveBlockReason(args)).toBeNull();
        expect(runtimeReadinessNotice(args)).toEqual({
            tone: 'ok',
            text: '远程直接运行依赖已就绪',
        });
    });

    it('missing and unsatisfied dependencies block start with catalog names', () => {
        const config = botConfig({ backend_type: 'snowluma' });
        const reason = runtimeStartBlockReason({
            config,
            componentNames: NAMES,
            local: {
                readiness: readiness('snowluma', ok, [
                    node('qq', { state: 'missing' }),
                    node('nodejs', {
                        state: 'unsatisfied',
                        found: '18.19.1',
                        reason: 'v18.19.1 不满足 ^22.13.0',
                    }),
                ]),
                probing: false,
            },
        });
        expect(reason).toBe(
            '本机缺少 QQ；不可用：Node.js（v18.19.1 不满足 ^22.13.0），请到「组件」页安装后再启动',
        );
    });

    it('root missing is reported first', () => {
        const config = botConfig();
        expect(
            runtimeStartBlockReason({
                config,
                componentNames: NAMES,
                local: {
                    readiness: readiness('napcat', { state: 'missing' }, [
                        node('qq', { state: 'missing' }),
                    ]),
                    probing: false,
                },
            }),
        ).toContain('缺少 NapCat、QQ');
    });

    it('unknown alone does not block but is surfaced as a neutral notice', () => {
        const config = botConfig();
        const args = {
            config,
            componentNames: NAMES,
            local: {
                readiness: readiness('napcat', ok, [
                    node('qq', { state: 'unknown', error: 'timeout' }),
                ]),
                probing: false,
            },
        };
        expect(runtimeStartBlockReason(args)).toBeNull();
        expect(runtimeReadinessNotice(args)).toEqual({
            tone: 'neutral',
            text: '本机未能确认 QQ，启动时再检查',
        });
    });

    it('still probing blocks start with a waiting message', () => {
        const config = botConfig();
        expect(
            runtimeStartBlockReason({ config, local: { readiness: undefined, probing: true } }),
        ).toContain('正在确认本机运行时组件');
        expect(runtimeStartBlockReason({ config })).toContain('正在检测本机运行时状态');
    });

    it('remote direct save is stricter: blocked dependencies refuse save', () => {
        const config = botConfig({
            runtime_target: 'server-a',
            backend_type: 'napcat',
            deploymentType: 'native',
        });
        const args = {
            config,
            componentNames: NAMES,
            remoteTransport: { reachable: true, label: 'kunming' },
            remoteDirect: {
                readiness: readiness('napcat', ok, [node('qq', { state: 'missing' })], 'remote:server-a'),
                probing: false,
            },
        };
        expect(runtimeStartBlockReason(args)).toContain('缺少 QQ');
        expect(runtimeSaveBlockReason(args)).toContain('缺少 QQ');
    });
});
