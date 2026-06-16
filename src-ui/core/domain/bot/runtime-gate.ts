// Bot 运行时启动门禁（本地 / 远程直接运行 / 远程 Docker 统一）。
//
// 目标：
// - 配置页选择底座 + 运行宿主 + 启动方式 时，立即给出是否可运行的提示。
// - 保存配置前阻断（saveBlock）。
// - 列表/卡片启动前阻断（startBlock）。
// - 远程探测有延迟时，显示“正在检测...”中性状态，不阻塞用户继续编辑。
//
// 三种启动模式对应探测源：
// 1. 本地直接运行 (Local + Native)          → hostId = 'local' 的 component detect
// 2. 远程直接运行 (Remote + Native)        → hostId = 'remote:${id}' 的 component detect
// 3. 远程 Docker (Remote + Docker)         → Docker 守护 + 镜像（复用 docker-start-gate）
//
// 直接运行所需组件由 remoteDirectRunChain 定义（NapCat: qq+napcat；SnowLuma: nodejs+qq+novnc+snowluma）。

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { BackendType } from '../../ipc/generated/domain/BackendType';
import {
    remoteDirectRunChain,
    localDirectRunChain,
    componentIdToDisplayName,
    type DirectRunComponentId,
} from './remote-direct-run-deps';
import { isRuntimeTargetLocal } from './runtime-target';

export type RuntimeRequirement =
    | { kind: 'local-direct'; backend: BackendType }
    | { kind: 'remote-direct'; hostId: string; backend: BackendType }
    | { kind: 'remote-docker'; hostId: string; backend: BackendType };

export function getRuntimeRequirement(config: BotConfig): RuntimeRequirement | null {
    const { backend_type, runtime_target, deploymentType } = config.bot;

    if (isRuntimeTargetLocal(runtime_target)) {
        // 本地只支持直接运行（Native）
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
    if (req.kind === 'remote-direct') {
        const label = req.backend === 'snowluma' ? 'SnowLuma' : 'NapCat';
        return `远程主机 ${req.hostId} 上的 ${label} 直接运行依赖`;
    }
    const label = req.backend === 'snowluma' ? 'SnowLuma' : 'NapCat';
    return `远程主机 ${req.hostId} 上的 ${label} Docker 镜像`;
}

// ========== 状态聚合 ==========

export interface LocalRuntimeStatus {
    installed: Partial<Record<DirectRunComponentId, boolean | undefined>>;
    probing: boolean;
}

export interface RemoteDirectStatus {
    installed: Partial<Record<DirectRunComponentId, boolean | undefined>>;
    probing: boolean;
}

export interface DockerStatusLite {
    installed: boolean;
    daemonRunning: boolean;
    composeAvailable: boolean;
    probing: boolean;
}

export interface RuntimeGateArgs {
    config: BotConfig;
    local?: LocalRuntimeStatus;
    remoteDirect?: RemoteDirectStatus; // 仅当 remote-direct 时使用
    docker?: DockerStatusLite;         // 仅当 remote-docker 时使用
}

/** 计算启动阻断原因（返回非空字符串表示不能启动）。 */
export function runtimeStartBlockReason(args: RuntimeGateArgs): string | null {
    const req = getRuntimeRequirement(args.config);
    if (!req) return null;

    if (req.kind === 'local-direct') {
        const st = args.local;
        if (!st) return '正在检测本机运行时状态...';
        const chain = localDirectRunChain(req.backend);
        const missing = chain.filter((id) => st.installed[id] === false);
        if (missing.length > 0) {
            return `本机缺少 ${missing.map(componentIdToDisplayName).join('、')}，请到「组件」页安装后再启动`;
        }
        if (chain.some((id) => st.installed[id] === undefined)) {
            return '正在确认本机运行时组件，请稍后再启动';
        }
        return null;
    }

    if (req.kind === 'remote-direct') {
        const st = args.remoteDirect;
        if (!st) return '正在检测远程主机组件...';
        const chain = remoteDirectRunChain(req.backend);
        const missing = chain.filter((id) => st.installed[id] === false);
        if (missing.length > 0) {
            return `远程主机缺少 ${missing.map(componentIdToDisplayName).join('、')}，请到「组件」页为该主机安装后再启动`;
        }
        if (chain.some((id) => st.installed[id] === undefined)) {
            return '正在确认远程主机组件，请稍后再启动';
        }
        return null;
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
        const st = args.remoteDirect;
        if (st && Object.values(st.installed).some((v) => v === false)) {
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

    if (req.kind === 'local-direct') {
        const st = args.local;
        if (!st || st.probing) {
            return { tone: 'neutral', text: '正在检测本机运行时组件...' };
        }
        const chain = localDirectRunChain(req.backend);
        const missing = chain.filter((id) => st.installed[id] === false);
        if (missing.length > 0) {
            return {
                tone: 'warn',
                text: `本机缺少 ${missing.map(componentIdToDisplayName).join('、')}，请到「组件」页安装后再启动`,
            };
        }
        return { tone: 'ok', text: '本机运行时组件已就绪' };
    }

    if (req.kind === 'remote-direct') {
        const st = args.remoteDirect;
        if (!st || st.probing) {
            return { tone: 'neutral', text: '正在检测远程主机组件...' };
        }
        const chain = remoteDirectRunChain(req.backend);
        const missing = chain.filter((id) => st.installed[id] === false);
        if (missing.length > 0) {
            return {
                tone: 'warn',
                text: `远程主机缺少 ${missing.map(componentIdToDisplayName).join('、')}，请到「组件」页安装`,
            };
        }
        return { tone: 'ok', text: '远程直接运行依赖已就绪' };
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
