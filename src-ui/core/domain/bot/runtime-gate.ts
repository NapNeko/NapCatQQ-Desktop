// Bot 启动门禁：配置页提示、保存阻断、列表启动阻断。
// 直接运行的组件状态来自后端 RuntimeReadiness，这里只决定文案与阻断。

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { BackendType } from '../../ipc/generated/domain/BackendType';
import type { RuntimeReadiness } from '../../ipc/generated/domain/RuntimeReadiness';
import {
    describeBlocking,
    targetDisplayName,
    unknownNodes,
    type ComponentNames,
} from '../components/readiness';
import { isRuntimeTargetLocal } from './runtime-target';

export type RuntimeRequirement =
    | { kind: 'local-direct'; backend: BackendType }
    | { kind: 'remote-direct'; hostId: string; backend: BackendType }
    | { kind: 'remote-docker'; hostId: string; backend: BackendType }
    | { kind: 'unsupported-local-docker'; backend: BackendType };

export function getRuntimeRequirement(config: BotConfig): RuntimeRequirement | null {
    const { backend_type, runtime_target, deploymentType } = config.bot;

    if (isRuntimeTargetLocal(runtime_target)) {
        if (deploymentType === 'docker') {
            return { kind: 'unsupported-local-docker', backend: backend_type };
        }
        return { kind: 'local-direct', backend: backend_type };
    }

    // 远程
    const hostId = `remote:${(runtime_target as any).server_id ?? (runtime_target as any)}`;
    if (deploymentType === 'docker') {
        return { kind: 'remote-docker', hostId, backend: backend_type };
    }
    return { kind: 'remote-direct', hostId, backend: backend_type };
}

/** 把 requirement 转成人类可读的“需要什么”描述（用于提示）。 */
export function describeRuntimeRequirement(req: RuntimeRequirement): string {
    if (req.kind === 'local-direct') {
        return req.backend === 'snowluma' ? '本机 SnowLuma 运行时' : '本机 NapCat 运行时';
    }
    if (req.kind === 'unsupported-local-docker') {
        return '本机 Docker 部署';
    }
    if (req.kind === 'remote-direct') {
        const label = req.backend === 'snowluma' ? 'SnowLuma' : 'NapCat';
        return `远程主机 ${req.hostId} 上的 ${label} 直接运行依赖`;
    }
    const label = req.backend === 'snowluma' ? 'SnowLuma' : 'NapCat';
    return `远程主机 ${req.hostId} 上的 ${label} Docker 镜像`;
}

// ========== 状态聚合 ==========

/** 直接运行（本机 / 远端）的框架 + 依赖状态 */
export interface DirectRuntimeStatus {
    readiness?: RuntimeReadiness | null;
    probing: boolean;
}

export interface DockerStatusLite {
    installed: boolean;
    daemonRunning: boolean;
    composeAvailable: boolean;
    probing: boolean;
}

/**
 * 远端主机的传输层可达性（仅 remote 场景使用）。
 * reachable=false 时，启动/保存门禁应优先阻断，并给出“主机连接中断”文案，
 * 而不是继续往下判断“缺少 NapCat/QQ”等组件级提示。
 */
export interface RemoteTransportStatus {
    reachable: boolean;
    /** 人类可读的主机标签（name · host 或 id），用于文案。 */
    label?: string;
}

export interface RuntimeGateArgs {
    config: BotConfig;
    local?: DirectRuntimeStatus;
    remoteDirect?: DirectRuntimeStatus; // 仅当 remote-direct 时使用
    docker?: DockerStatusLite;         // 仅当 remote-docker 时使用
    /** 仅当涉及远端主机时提供；useBotRuntimeStartGate 负责填充。 */
    remoteTransport?: RemoteTransportStatus;
    /** 组件 id → 显示名（catalog）；缺省显示 id */
    componentNames?: ComponentNames;
}

function directBlockReason(
    st: DirectRuntimeStatus | undefined,
    where: '本机' | '远程主机',
    names: ComponentNames | undefined,
): string | null {
    if (!st) return `正在检测${where}运行时状态...`;
    if (!st.readiness) {
        return st.probing ? `正在确认${where}运行时组件，请稍后再启动` : null;
    }
    const blocking = describeBlocking(st.readiness, names);
    if (!blocking) return null;
    const hint = where === '本机' ? '请到「组件」页安装后再启动' : '请到「组件」页为该主机安装后再启动';
    return `${where}${blocking}，${hint}`;
}

/** 计算启动阻断原因（返回非空字符串表示不能启动）。 */
export function runtimeStartBlockReason(args: RuntimeGateArgs): string | null {
    const req = getRuntimeRequirement(args.config);
    if (!req) return null;

    if (req.kind === 'unsupported-local-docker') {
        return '本机不支持 Docker 部署，请改为直接运行或选择远程主机';
    }

    // 优先检查传输层：远端主机不可达时，直接返回 transport 原因，不再看组件。
    if ((req.kind === 'remote-direct' || req.kind === 'remote-docker') && args.remoteTransport) {
        if (!args.remoteTransport.reachable) {
            const label = args.remoteTransport.label ?? req.hostId;
            return `远端主机 ${label} 连接中断`;
        }
    }

    if (req.kind === 'local-direct') {
        return directBlockReason(args.local, '本机', args.componentNames);
    }

    if (req.kind === 'remote-direct') {
        return directBlockReason(args.remoteDirect, '远程主机', args.componentNames);
    }

    // remote-docker 复用现有 docker 逻辑（这里只做简单兜底，真实阻断仍由 docker 门禁主负责）
    const d = args.docker;
    if (!d) return '正在检测 Docker 状态...';
    if (!d.installed || !d.daemonRunning || !d.composeAvailable) {
        return '远程主机 Docker 未就绪，请到「组件」页安装并启动 Docker';
    }
    return null;
}

/** 保存配置时阻断原因（比启动更严格一些，远程 direct 缺依赖不允许保存）。 */
export function runtimeSaveBlockReason(args: RuntimeGateArgs): string | null {
    const block = runtimeStartBlockReason(args);
    if (block) return block;

    const req = getRuntimeRequirement(args.config);
    if (req?.kind === 'remote-direct') {
        const readiness = args.remoteDirect?.readiness;
        if (readiness && describeBlocking(readiness, args.componentNames)) {
            return '远程直接运行依赖不完整，保存后也无法启动。请先安装缺失组件。';
        }
    }
    return null;
}

/** 配置页显示的就绪提示（中性/成功/警告）。 */
export function runtimeReadinessNotice(args: RuntimeGateArgs): {
    tone: 'ok' | 'warn' | 'neutral';
    text: string;
} | null {
    const req = getRuntimeRequirement(args.config);
    if (!req) return null;

    if (req.kind === 'unsupported-local-docker') {
        return {
            tone: 'warn',
            text: '本机不支持 Docker 部署，请改为直接运行或选择远程主机',
        };
    }

    if (req.kind === 'local-direct' || req.kind === 'remote-direct') {
        const where = req.kind === 'local-direct' ? '本机' : '远程主机';
        const st = req.kind === 'local-direct' ? args.local : args.remoteDirect;
        if (!st || st.probing || !st.readiness) {
            return { tone: 'neutral', text: `正在检测${where}运行时组件...` };
        }
        const blocking = describeBlocking(st.readiness, args.componentNames);
        if (blocking) {
            return { tone: 'warn', text: `${where}${blocking}，请到「组件」页安装` };
        }
        const unknown = unknownNodes(st.readiness);
        if (unknown.length) {
            const names = unknown
                .map((n) => targetDisplayName(n.target, args.componentNames))
                .join('、');
            return { tone: 'neutral', text: `${where}未能确认 ${names}，启动时再检查` };
        }
        return {
            tone: 'ok',
            text: where === '本机' ? '本机运行时组件已就绪' : '远程直接运行依赖已就绪',
        };
    }

    // docker 由 dockerReadinessNotice 主导，这里只做兜底
    const d = args.docker;
    if (!d || d.probing) {
        return { tone: 'neutral', text: '正在检测 Docker...' };
    }
    if (!d.installed || !d.daemonRunning || !d.composeAvailable) {
        return { tone: 'warn', text: '远程主机 Docker 未就绪，请到「组件」页安装' };
    }
    return { tone: 'ok', text: 'Docker 已就绪' };
}
