// 应用实例状态的展示元数据（列表行与详情页头部共用），以及从任务表认出某个实例的安装任务。

import type {
    AppInstance,
    AppInstanceState,
    DeploymentTaskSnapshot,
} from '../../ipc/types';

export const STATE_META: Record<
    AppInstanceState,
    { label: string; tone: 'neutral' | 'info' | 'brand' | 'success' | 'warning'; dot?: boolean }
> = {
    not_installed: { label: '未安装', tone: 'neutral' },
    installing: { label: '安装中', tone: 'info' },
    installed: { label: '已安装', tone: 'brand' },
    running: { label: '运行中', tone: 'success', dot: true },
    stopped: { label: '已停止', tone: 'warning' },
};

export function isInstalled(i: AppInstance): boolean {
    return i.state !== 'not_installed' && i.state !== 'installing';
}

/** 在跑或在装：这时候把应用端整块藏起来，实例就没处看也没处停了 */
export function isInstanceActive(i: AppInstance): boolean {
    return i.state === 'running' || i.state === 'installing';
}

const INSTALL_ACTIONS = new Set(['ensure_installed', 'force_install', 'update']);

function appInstallTaskTarget(instance: AppInstance): string {
    return `${instance.framework_id}@${instance.id}`;
}

export function matchesAppInstallTask(task: DeploymentTaskSnapshot, instance: AppInstance): boolean {
    if (task.kind.kind !== 'component_action') return false;
    if (!INSTALL_ACTIONS.has(task.kind.action)) return false;
    const expect = appInstallTaskTarget(instance);
    return task.resources.some(
        (resource) =>
            resource.kind === 'install_target'
            && (resource.target === expect || resource.target === instance.install_dir),
    );
}

export function latestAppInstallTask(
    tasks: Record<string, DeploymentTaskSnapshot>,
    instance: AppInstance,
): DeploymentTaskSnapshot | undefined {
    let latest: DeploymentTaskSnapshot | undefined;
    for (const task of Object.values(tasks)) {
        if (!matchesAppInstallTask(task, instance)) continue;
        if (!latest || Number(task.submittedAtMs) > Number(latest.submittedAtMs)) latest = task;
    }
    return latest;
}
