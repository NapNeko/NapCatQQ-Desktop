// Bot 列表启动前：Docker 镜像须在组件页预拉。

import { useCallback, useMemo } from 'react';
import { dockerBotTarget, dockerHostTargetKey } from '../../core/domain/docker/engine';
import { useDockerHosts } from '../docker/useDockerHosts';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import {
    dockerHostIdForConfig,
    dockerFlavorForBackend,
    dockerStartBlockReason,
    dockerSaveBlockReason,
} from '../../core/domain/bot/docker-start-gate';

export function useBotDockerStartGate(configByBot: Record<string, BotConfig | undefined | null>): {
    startBlock: (botId: string) => string | null;
    saveBlock: (config: BotConfig) => string | null;
} {
    const hostIds = useMemo(() => {
        const set = new Map<string, ReturnType<typeof dockerBotTarget>>();
        for (const c of Object.values(configByBot)) {
            if (!c) continue;
            const h = dockerHostIdForConfig(c);
            if (h) {
                const target = dockerBotTarget(c, h);
                set.set(dockerHostTargetKey(target), target);
            }
        }
        return [...set.values()];
    }, [configByBot]);

    const { statusByHost, probingByHost, imageReadyByHost } = useDockerHosts(hostIds);

    const gateArgs = useCallback(
        (config: BotConfig) => {
            const hostId = dockerHostIdForConfig(config);
            if (!hostId) return null;
            const key = dockerHostTargetKey(dockerBotTarget(config, hostId));
            const flavor = dockerFlavorForBackend(config.bot.backend_type);
            return {
                config,
                dockerStatus: statusByHost[key],
                imageReady: imageReadyByHost[key]?.[flavor],
                dockerProbing: probingByHost[key],
            };
        },
        [statusByHost, imageReadyByHost, probingByHost],
    );

    const startBlock = useCallback(
        (botId: string) => {
            const config = configByBot[botId];
            if (!config) return null;
            const args = gateArgs(config);
            if (!args) return null;
            return dockerStartBlockReason(args);
        },
        [configByBot, gateArgs],
    );

    const saveBlock = useCallback(
        (config: BotConfig) => {
            const args = gateArgs(config);
            if (!args) return null;
            return dockerSaveBlockReason(args);
        },
        [gateArgs],
    );

    return { startBlock, saveBlock };
}
