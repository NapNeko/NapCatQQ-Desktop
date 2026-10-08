// 组件页 QQ 依赖探测的门禁与状态，外加「失败任务里找 sudo 提权」的扫描。
// 探测只对 Linux 且 QQ 已装好的主机有意义；错误文案由调用方经 errorText 生成。

import type { ActionProgressView } from './progress';
import type { MachineView } from './types';
import type { ComponentId } from '../../ipc/types';
import type { QqDependencyReport } from '../../ipc/generated/qq/QqDependencyReport';

export type QqDependencyProbeState =
    | { status: 'loading'; report: null; error: null }
    | { status: 'ready'; report: QqDependencyReport; error: null }
    | { status: 'error'; report: null; error: string };

export function canProbeQqDependencies(
    machine: MachineView | null | undefined,
): machine is MachineView {
    if (!machine || machine.host.os !== 'linux') return false;
    const qq = machine.runtimeDep.find((row) => row.info.id === 'qq');
    return qq?.status.state === 'installed';
}

// sudo 密码弹框的目标；docker 安装与 QQ 依赖修复共用一个弹框，purpose 分流重试路径。
export type SudoPromptPurpose = 'docker' | 'qq_deps';

export interface SudoPromptTarget {
    hostId: string;
    hostName: string;
    reason?: string;
    purpose: SudoPromptPurpose;
}

export interface QqSudoElevationHit {
    taskId: string;
    hostId: string;
    reason: string;
}

// 扫终态任务，找第一个「QQ 组件因缺提权而失败、且还没提示过」的 task。
// 后端在日志里埋 elevation_required 标记；取最后一条 error 日志、没有就退回 message。
export function findQqSudoElevationTask(
    tasks: Readonly<Record<string, ActionProgressView>>,
    taskTargets: Readonly<Record<string, { componentId: ComponentId; hostId: string }>>,
    alreadyPrompted: ReadonlySet<string>,
): QqSudoElevationHit | null {
    for (const [taskId, progress] of Object.entries(tasks)) {
        if (progress.status !== 'failed') continue;
        if (alreadyPrompted.has(taskId)) continue;
        const target = taskTargets[taskId];
        if (!target || target.componentId !== 'qq') continue;
        const msg =
            [...progress.logs].reverse().find((l) => l.level === 'error')?.message ??
            progress.message;
        if (!msg.includes('elevation_required')) continue;
        return { taskId, hostId: target.hostId, reason: msg };
    }
    return null;
}
