// Bot 运行时启动/保存门禁 hook。
// 直接运行的框架 + 依赖状态由后端 resolve_runtime_readiness 给出，这里只按 (host, backend) 取一次。

import { useCallback, useMemo } from 'react';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import type { BackendType } from '../../core/ipc/generated/domain/BackendType';
import {
    getRuntimeRequirement,
    runtimeSaveBlockReason,
    runtimeStartBlockReason,
    type RuntimeGateArgs,
    type RemoteTransportStatus,
} from '../../core/domain/bot/runtime-gate';
import { useComponentNames, useRuntimeReadinessMany } from '../components/useRuntimeReadiness';
import { useDockerHosts } from '../docker/useDockerHosts';
import { dockerHostIdForConfig } from '../../core/domain/bot/docker-start-gate';
import { useQuery } from '@tanstack/react-query';
import { serverService } from '../../core/services/server.service';
import { isTauri } from '../../core/ipc/transport';
import { isHostReachableFromCache } from '../remote/useIsHostReachable';

export function useBotRuntimeStartGate(
    configByBot: Record<string, BotConfig | undefined | null>,
): {
    startBlock: (botId: string) => string | null;
    saveBlock: (config: BotConfig) => string | null;
} {
    const serversQuery = useQuery({
        queryKey: ['servers'],
        queryFn: () => serverService.list(),
        enabled: isTauri,
        staleTime: 15_000,
    });
    const servers = serversQuery.data ?? [];

    // 直接运行的 (host, backend) 对；主机不可达时不发探测
    const directTargets = useMemo(() => {
        const seen = new Map<string, { hostId: string; backend: BackendType; enabled: boolean }>();
        for (const c of Object.values(configByBot)) {
            if (!c) continue;
            const req = getRuntimeRequirement(c);
            if (!req) continue;
            const hostId =
                req.kind === 'local-direct' ? 'local' : req.kind === 'remote-direct' ? req.hostId : null;
            if (!hostId) continue;
            const key = `${hostId}|${req.backend}`;
            if (!seen.has(key)) {
                seen.set(key, {
                    hostId,
                    backend: req.backend,
                    enabled: isHostReachableFromCache(hostId, servers),
                });
            }
        }
        return [...seen.values()];
    }, [configByBot, servers]);
    const readinessByTarget = useRuntimeReadinessMany(directTargets);
    const componentNames = useComponentNames();

    const dockerHostIds = useMemo(() => {
        const hosts = new Set<string>();
        for (const c of Object.values(configByBot)) {
            const h = c ? dockerHostIdForConfig(c) : null;
            if (h) hosts.add(h);
        }
        return [...hosts];
    }, [configByBot]);
    const { statusByHost: dockerStatusByHost, probingByHost: dockerProbingByHost } =
        useDockerHosts(dockerHostIds);

    const transportFor = useCallback(
        (hostId: string): RemoteTransportStatus => {
            const serverId = hostId.startsWith('remote:') ? hostId.slice('remote:'.length) : hostId;
            const profile = servers.find((p) => p.id === serverId);
            return {
                reachable: profile ? profile.state !== 'failed' : false,
                label: profile
                    ? profile.name?.trim() || profile.host?.trim() || profile.id
                    : serverId,
            };
        },
        [servers],
    );

    const gateArgs = useCallback(
        (config: BotConfig): RuntimeGateArgs => {
            const req = getRuntimeRequirement(config);
            const out: RuntimeGateArgs = { config, componentNames };
            if (!req) return out;

            if (req.kind === 'local-direct') {
                const st = readinessByTarget[`local|${req.backend}`];
                out.local = { readiness: st?.readiness, probing: st?.probing ?? true };
            } else if (req.kind === 'remote-direct') {
                const st = readinessByTarget[`${req.hostId}|${req.backend}`];
                out.remoteDirect = { readiness: st?.readiness, probing: st?.probing ?? true };
                out.remoteTransport = transportFor(req.hostId);
            } else if (req.kind === 'remote-docker') {
                const dockerStatus = dockerStatusByHost[req.hostId];
                out.docker = {
                    installed: !!dockerStatus?.installed,
                    daemonRunning: !!dockerStatus?.daemonRunning,
                    composeAvailable: !!dockerStatus?.composeAvailable,
                    probing: dockerProbingByHost[req.hostId] ?? false,
                };
                out.remoteTransport = transportFor(req.hostId);
            }
            return out;
        },
        [readinessByTarget, componentNames, dockerStatusByHost, dockerProbingByHost, transportFor],
    );

    const startBlock = useCallback(
        (botId: string) => {
            const config = configByBot[botId];
            if (!config) return null;
            return runtimeStartBlockReason(gateArgs(config));
        },
        [configByBot, gateArgs],
    );

    const saveBlock = useCallback(
        (config: BotConfig) => runtimeSaveBlockReason(gateArgs(config)),
        [gateArgs],
    );

    return { startBlock, saveBlock };
}
