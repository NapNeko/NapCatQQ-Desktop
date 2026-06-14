// SnowLuma 列表/门禁：本机、远端 Native、远端 Docker 三种运行形态。

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { DaemonState } from '../../ipc/generated/DaemonState';
import { isSnowLumaFlavor } from './flavor';
import { isRuntimeTargetConcreteRemote } from './runtime-target';

export function isSnowlumaRemoteNativeConfig(config: BotConfig | null | undefined): boolean {
    if (!config) return false;
    if (!isSnowLumaFlavor(config.bot.backend_type)) return false;
    if (config.bot.deploymentType !== 'native') return false;
    return isRuntimeTargetConcreteRemote(config.bot.runtime_target);
}

export function isSnowlumaRemoteDockerConfig(config: BotConfig | null | undefined): boolean {
    if (!config) return false;
    if (!isSnowLumaFlavor(config.bot.backend_type)) return false;
    return config.bot.deploymentType === 'docker';
}

/** WebUI / noVNC 隧道是否可认为就绪（Docker 事件或远端 Native 全局 daemon Ready）。 */
export function isSnowlumaTunnelReady(args: {
    config: BotConfig | null | undefined;
    dockerEndpointsReady: boolean;
    daemonState: DaemonState | null | undefined;
}): boolean {
    if (args.dockerEndpointsReady) return true;
    if (!isSnowlumaRemoteNativeConfig(args.config)) return false;
    return args.daemonState === 'ready';
}