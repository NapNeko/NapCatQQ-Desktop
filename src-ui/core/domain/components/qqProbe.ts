// 组件页 QQ 依赖探测的门禁与状态，外加「失败任务里找 sudo 提权」的扫描。
// 探测只对 Linux 且 QQ 已装好的主机有意义；错误文案由调用方经 errorText 生成。

import type { ActionProgressView } from './progress';
import type { MachineView } from './types';
import type { ComponentId, SnowLumaLinuxPackage, StepKind } from '../../ipc/types';
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

interface SudoPromptHost {
    hostId: string;
    hostName: string;
    reason?: string;
}

export type QqSudoActionKind = Extract<
    StepKind,
    'ensure_installed' | 'force_install' | 'update' | 'ensure_dependencies'
>;

export type QqSudoComponentId = Extract<ComponentId, 'qq' | 'napcat' | 'snowluma' | 'novnc'>;

export type SudoPromptTarget = SudoPromptHost &
    (
        | { purpose: 'docker' }
        | {
              purpose: 'qq_action';
              componentId: QqSudoComponentId;
              kind: QqSudoActionKind;
              taskId: string;
              snowlumaLinuxPackage?: SnowLumaLinuxPackage;
          }
    );

export function isQqSudoAction(kind: StepKind | undefined): kind is QqSudoActionKind {
    return (
        kind === 'ensure_installed' ||
        kind === 'force_install' ||
        kind === 'update' ||
        kind === 'ensure_dependencies'
    );
}

export function qqSudoActionLabel(
    kind: QqSudoActionKind,
    componentId: QqSudoComponentId = 'qq',
): string {
    const name =
        componentId === 'napcat'
            ? 'NapCat'
            : componentId === 'snowluma'
              ? 'SnowLuma'
              : componentId === 'novnc'
                ? 'noVNC'
                : 'QQ';
    return kind === 'update'
        ? `更新 ${name}`
        : kind === 'ensure_dependencies'
          ? `修复 ${name} 依赖`
          : `安装 ${name}`;
}

export interface QqSudoElevationHit {
    componentId: QqSudoComponentId;
    snowlumaLinuxPackage?: SnowLumaLinuxPackage;
    taskId: string;
    hostId: string;
    reason: string;
    kind: QqSudoActionKind;
}

// 扫协议端安装的终态任务，保留因 QQ 依赖缺提权而失败的原组件和包型。
// finished 会追加不含提权标记的「任务失败」，因此要查找最近一条含标记的错误。
export function findQqSudoElevationTask(
    tasks: Readonly<Record<string, ActionProgressView>>,
    taskTargets: Readonly<
        Record<
            string,
            {
                componentId: ComponentId;
                hostId: string;
                kind?: StepKind;
                snowlumaLinuxPackage?: SnowLumaLinuxPackage;
            }
        >
    >,
    alreadyPrompted: ReadonlySet<string>,
): QqSudoElevationHit | null {
    for (const [taskId, progress] of Object.entries(tasks)) {
        if (progress.status !== 'failed') continue;
        if (alreadyPrompted.has(taskId)) continue;
        const target = taskTargets[taskId];
        if (
            !target ||
            (target.componentId !== 'qq' &&
                target.componentId !== 'napcat' &&
                target.componentId !== 'snowluma' &&
                target.componentId !== 'novnc') ||
            (target.hostId !== 'local' && !target.hostId.startsWith('remote:')) ||
            !isQqSudoAction(target.kind)
        )
            continue;
        const msg =
            [...progress.logs]
                .reverse()
                .find((l) => l.level === 'error' && l.message.includes('elevation_required'))
                ?.message ?? progress.message;
        if (!msg.includes('elevation_required')) continue;
        const reason = msg
            .slice(msg.indexOf('elevation_required') + 'elevation_required'.length)
            .replace(/^:\s*/, '');
        return {
            taskId,
            hostId: target.hostId,
            reason,
            kind: target.kind,
            componentId: target.componentId,
            ...(target.snowlumaLinuxPackage === undefined
                ? {}
                : { snowlumaLinuxPackage: target.snowlumaLinuxPackage }),
        };
    }
    return null;
}
