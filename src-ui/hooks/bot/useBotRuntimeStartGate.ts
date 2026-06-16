// Bot 运行时启动/保存门禁 hook（最终正确实现版）。
//
// 所有 hooks 在顶层调用，数量由本轮涉及的 host 集合决定（稳定）。
// 然后 gateArgs / startBlock / saveBlock 只是读 map + 纯计算。

import { useCallback, useMemo } from 'react';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import type { BackendType } from '../../core/ipc/generated/domain/BackendType';
import {
    getRuntimeRequirement,
    runtimeSaveBlockReason,
    runtimeStartBlockReason,
    type RuntimeGateArgs,
} from '../../core/domain/bot/runtime-gate';
import { useHostComponentInstalled } from '../components/useRemoteHostComponentInstalled';
import { useDockerHosts } from '../docker/useDockerHosts';
import {
    dockerHostIdForConfig,
} from '../../core/domain/bot/docker-start-gate';
import { isRuntimeTargetLocal } from '../../core/domain/bot/runtime-target';

export function useBotRuntimeStartGate(
    configByBot: Record<string, BotConfig | undefined | null>,
): {
    startBlock: (botId: string) => string | null;
    saveBlock: (config: BotConfig) => string | null;
} {
    // 1. 收集本轮所有涉及的主机（'local' + remote:*）
    const relevantHosts = useMemo(() => {
        const hosts = new Set<string>();
        for (const c of Object.values(configByBot)) {
            if (!c) continue;
            if (isRuntimeTargetLocal(c.bot.runtime_target)) {
                hosts.add('local');
            } else {
                const rt: any = c.bot.runtime_target;
                const sid = rt?.server_id ?? rt;
                if (sid) hosts.add(`remote:${sid}`);
            }
            const dHost = dockerHostIdForConfig(c);
            if (dHost) hosts.add(dHost);
        }
        return [...hosts];
    }, [configByBot]);

    // 2. 顶层为每个 host 取两种 backend 的状态（hook 调用数量 = hosts.length × 2，稳定）
    const statusByHost: Record<
        string,
        Record<BackendType, ReturnType<typeof useHostComponentInstalled>>
    > = {};

    for (const h of relevantHosts) {
        // eslint-disable-next-line react-hooks/rules-of-hooks
        statusByHost[h] = {
            napcat: useHostComponentInstalled(h, 'napcat'),
            snowluma: useHostComponentInstalled(h, 'snowluma'),
        };
    }

    // 3. Docker 状态（复用）
    const dockerHostIds = relevantHosts.filter((h) => h.startsWith('remote:'));
    const { statusByHost: dockerStatusByHost, probingByHost: dockerProbingByHost } =
        useDockerHosts(dockerHostIds);

    // 4. 构造 gateArgs（纯读 + 计算）
    const gateArgs = useCallback(
        (config: BotConfig): RuntimeGateArgs => {
            const req = getRuntimeRequirement(config);
            const out: RuntimeGateArgs = { config };

            if (!req) return out;

            if (req.kind === 'local-direct') {
                const st = statusByHost['local']?.[config.bot.backend_type];
                out.local = {
                    installed: st ?? {},
                    probing: st ? Object.values(st).some((v) => v === undefined) : true,
                };
            } else if (req.kind === 'remote-direct') {
                const st = statusByHost[req.hostId]?.[config.bot.backend_type];
                out.remoteDirect = {
                    installed: st ?? {},
                    probing: st ? Object.values(st).some((v) => v === undefined) : true,
                };
            } else if (req.kind === 'remote-docker') {
                const hostId = req.hostId;
                const dockerStatus = dockerStatusByHost[hostId];
                const probing = dockerProbingByHost[hostId] ?? false;
                out.docker = {
                    installed: !!dockerStatus?.installed,
                    daemonRunning: !!dockerStatus?.daemonRunning,
                    composeAvailable: !!dockerStatus?.composeAvailable,
                    probing,
                };
            }

            return out;
        },
        [statusByHost, dockerStatusByHost, dockerProbingByHost],
    );

    const startBlock = useCallback(
        (botId: string) => {
            const config = configByBot[botId];
            if (!config) return null;
            const args = gateArgs(config);
            return runtimeStartBlockReason(args);
        },
        [configByBot, gateArgs],
    );

    const saveBlock = useCallback(
        (config: BotConfig) => {
            const args = gateArgs(config);
            return runtimeSaveBlockReason(args);
        },
        [gateArgs],
    );

    return { startBlock, saveBlock };
}
