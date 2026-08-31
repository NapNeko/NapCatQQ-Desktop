// SnowLuma 列表/门禁：本机、远端 Native、远端 Docker 三种运行形态。

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { DaemonState } from '../../ipc/generated/DaemonState';
import type { SnowLumaLoginState } from '../../ipc/generated/SnowLumaLoginState';
import { isSnowLumaFlavor } from './flavor';
import {
    isRuntimeTargetConcreteRemote,
    normalizeRuntimeTargetFromDisk,
} from './runtime-target';

/** SnowLuma Native daemon 的运行作用域；Docker 每 Bot 独立，不走 daemon。 */
export function snowlumaDaemonScope(
    config: BotConfig | null | undefined,
): string | null {
    if (!config) return null;
    if (!isSnowLumaFlavor(config.bot.backend_type)) return null;
    if (config.bot.deploymentType !== 'native') return null;
    return normalizeRuntimeTargetFromDisk(config.bot.runtime_target);
}

export function snowlumaDaemonStateForConfig(
    config: BotConfig | null | undefined,
    daemonStates: Record<string, DaemonState>,
): DaemonState | null {
    const scope = snowlumaDaemonScope(config);
    return scope ? (daemonStates[scope] ?? null) : null;
}

export function isSnowlumaRemoteNativeConfig(config: BotConfig | null | undefined): boolean {
    if (!config) return false;
    if (!isSnowLumaFlavor(config.bot.backend_type)) return false;
    if (config.bot.deploymentType !== 'native') return false;
    return isRuntimeTargetConcreteRemote(config.bot.runtime_target);
}

export function isSnowlumaQrActionAvailable(
    config: BotConfig | null | undefined,
    active: boolean,
    loginState: SnowLumaLoginState | null | undefined,
): boolean {
    return active && loginState !== 'logged_in' && isSnowlumaRemoteNativeConfig(config);
}

export function isSnowlumaRemoteDockerConfig(config: BotConfig | null | undefined): boolean {
    if (!config) return false;
    if (!isSnowLumaFlavor(config.bot.backend_type)) return false;
    return config.bot.deploymentType === 'docker';
}

/** 远端 SnowLuma 运行中时始终保留一次隧道重建入口，避免失效缓存只能靠重启恢复。 */
export function isSnowlumaRemoteUiRetryAvailable(args: {
    config: BotConfig | null | undefined;
    active: boolean;
    transportFailed: boolean;
}): boolean {
    if (!args.active || args.transportFailed) return false;
    return (
        isSnowlumaRemoteNativeConfig(args.config) ||
        isSnowlumaRemoteDockerConfig(args.config)
    );
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
