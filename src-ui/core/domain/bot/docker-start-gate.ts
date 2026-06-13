// Bot Docker 启动前门禁：镜像须在「组件」页预拉，启动时不现场 pull。

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { DockerFlavor } from '../../ipc/generated/domain/DockerFlavor';
import type { DockerStatus } from '../../ipc/types';
import { isRuntimeTargetConcreteRemote } from './runtime-target';

const FLAVOR_LABEL: Record<DockerFlavor, string> = {
    napcat: 'NapCat',
    snowluma: 'SnowLuma',
};

export function dockerFlavorForBackend(
    backend: BotConfig['bot']['backend_type'],
): DockerFlavor {
    return backend === 'snowluma' ? 'snowluma' : 'napcat';
}

export function dockerHostIdForConfig(config: BotConfig): string | null {
    const t = config.bot.runtime_target;
    if (!isRuntimeTargetConcreteRemote(t)) return null;
    return `remote:${t}`;
}

export function isDockerBotConfig(config: BotConfig): boolean {
    return (
        config.bot.deploymentType === 'docker' &&
        isRuntimeTargetConcreteRemote(config.bot.runtime_target)
    );
}

function dockerImageGateReason(
    args: {
        config: BotConfig;
        dockerStatus: DockerStatus | undefined;
        imageReady: boolean | undefined;
        dockerProbing?: boolean;
    },
    when: 'start' | 'save',
): string | null {
    const { config, dockerStatus, imageReady, dockerProbing } = args;
    if (!isDockerBotConfig(config)) return null;

    const label =
        FLAVOR_LABEL[dockerFlavorForBackend(config.bot.backend_type)];
    const later = when === 'save' ? '后再保存' : '后再启动';

    if (dockerProbing && !dockerStatus) {
        return when === 'save'
            ? '正在检查 Docker 状态，请稍后再保存'
            : '正在检查 Docker 状态，请稍后再启动';
    }

    const dockerOk =
        dockerStatus?.installed &&
        dockerStatus.daemonRunning &&
        dockerStatus.composeAvailable;

    if (!dockerOk) {
        return '此主机 Docker 未就绪，请到「组件」页安装并启动 Docker';
    }

    if (imageReady === false) {
        return `${label} 镜像未拉取，请到「组件」页拉取镜像${later}`;
    }

    if (imageReady !== true) {
        return `正在确认 ${label} 镜像，请稍${later}`;
    }

    return null;
}

/** 配置页 / 列表启动拦截用；返回 null 表示可启动。 */
export function dockerStartBlockReason(args: {
    config: BotConfig;
    dockerStatus: DockerStatus | undefined;
    imageReady: boolean | undefined;
    dockerProbing?: boolean;
}): string | null {
    return dockerImageGateReason(args, 'start');
}

/** 保存 Bot 配置前拦截（远程 Docker 且镜像未就绪时不允许保存）。 */
export function dockerSaveBlockReason(args: {
    config: BotConfig;
    dockerStatus: DockerStatus | undefined;
    imageReady: boolean | undefined;
    dockerProbing?: boolean;
}): string | null {
    return dockerImageGateReason(args, 'save');
}

/** 配置页 Docker 就绪条（与启动门禁文案一致）。 */
export function dockerReadinessNotice(args: {
    flavorLabel: string;
    status: DockerStatus | undefined;
    probing: boolean;
    imageReady: boolean | undefined;
}): { tone: 'ok' | 'warn' | 'neutral'; text: string } | null {
    const { flavorLabel, status, probing, imageReady } = args;

    if (probing && !status) {
        return { tone: 'neutral', text: '正在检查 Docker…' };
    }

    const dockerOk =
        status?.installed && status.daemonRunning && status.composeAvailable;

    if (!dockerOk) {
        return {
            tone: 'warn',
            text: '此主机 Docker 未就绪，请到「组件」页安装并启动 Docker',
        };
    }

    if (imageReady === false) {
        return {
            tone: 'warn',
            text: `${flavorLabel} 镜像未拉取，请到「组件」页拉取镜像`,
        };
    }

    if (imageReady === true) {
        const ver = status.version?.trim();
        return {
            tone: 'ok',
            text: `${ver ? `Docker ${ver} · ` : ''}${flavorLabel} 镜像已就绪，可启动`,
        };
    }

    return {
        tone: 'neutral',
        text: `Docker 已就绪，正在确认 ${flavorLabel} 镜像…`,
    };
}