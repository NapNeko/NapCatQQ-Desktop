import type { ContainerEngine } from '../../ipc/generated/domain/ContainerEngine';
import type { BotConfig } from '../../ipc/generated/domain/BotConfig';

export interface DockerHostTarget {
    hostId: string;
    engine?: ContainerEngine;
}

export function dockerHostTarget(target: string | DockerHostTarget): DockerHostTarget {
    return typeof target === 'string' ? { hostId: target } : target;
}

export function dockerHostTargetKey(target: string | DockerHostTarget): string {
    const { hostId, engine } = dockerHostTarget(target);
    return engine ? `${hostId}::${engine}` : hostId;
}

export function dockerBotTarget(config: BotConfig, hostId: string): DockerHostTarget {
    return { hostId, engine: config.docker?.engine };
}

export function dockerQueryMatchesHost(key: readonly unknown[], hostId: string): boolean {
    return (
        key[0] === 'docker' &&
        typeof key[2] === 'string' &&
        (key[2] === hostId || key[2] === `${hostId}::docker` || key[2] === `${hostId}::podman`)
    );
}
