// 假装的安装流水线：按真机 ensure_installed 的事件顺序回放一遍进度。
import type {
    AppInstance,
    DeploymentTaskSnapshot,
    DomainEvent,
    ProgressEvent,
    ProgressKind,
} from '../../types';
import { latestStable } from '../../../domain/apps/appVersions';
import { emitMockEvent } from '../events.mock';
import { publish, require } from './state';

const MOCK_INSTALL_STEPS = [
    '解析 uv',
    '下载源码',
    '放置源码',
    '同步 Python 依赖',
    '预置端口与协议确认',
];

// NeoBot 的发行清单，最新在前；listVersions 与「装到最新正式版」的回填共用同一份
export const MOCK_NEOBOT_VERSIONS = ['1.2.1', '1.2.0', '1.1.0', '1.0.0', '1.0.0a25'];

/**
 * 按真机的事件顺序假装跑一次安装：任务快照、步骤进度、最后实例变成已安装。
 * 走查卡片 / 详情页的「安装中」进度用。
 */
export function simulateInstallTask(inst: AppInstance, version: string | null = null): string {
    const taskId = `mock-install-${inst.id}-${Date.now()}`;
    const target = `${inst.framework_id}@${inst.id}`;
    const submittedAtMs = BigInt(Date.now());
    let events: ProgressEvent[] = [];
    const snapshot = (status: DeploymentTaskSnapshot['status']): DeploymentTaskSnapshot => ({
        taskId,
        kind: {
            kind: 'component_action',
            component_id: inst.framework_id,
            action: 'ensure_installed',
        },
        status,
        hostId: inst.host_id,
        title: `${target} ensure_installed`,
        resources: [{ kind: 'install_target', host_id: inst.host_id, target }],
        progressEvents: events,
        submittedAtMs,
        cancellable: true,
    });
    const push = (kind: ProgressKind) => {
        const event = { v: 1, timestamp_ms: BigInt(Date.now()), ...kind } as ProgressEvent;
        events = [...events, event];
        emitMockEvent({ kind: 'component_action_progress', task_id: taskId, event } as DomainEvent);
        emitMockEvent({
            kind: 'deployment_task_changed',
            task: snapshot('running'),
        } as DomainEvent);
    };

    emitMockEvent({ kind: 'deployment_task_changed', task: snapshot('queued') } as DomainEvent);
    const plan: Array<() => void> = [
        () => push({ kind: 'started', total_steps: MOCK_INSTALL_STEPS.length }),
    ];
    MOCK_INSTALL_STEPS.forEach((message, idx) => {
        const step = idx + 1;
        plan.push(() => push({ kind: 'step_begin', step, message }));
        plan.push(() => push({ kind: 'step_end', step, ok: true }));
    });
    plan.push(() => push({ kind: 'finished', ok: true }));
    plan.push(() => {
        emitMockEvent({
            kind: 'deployment_task_changed',
            task: snapshot('success'),
        } as DomainEvent);
        // 指定了版本就回填它——真实链路由 detect 从发行元数据回读，预览里照同样的口径表现。
        // NeoBot 不指定版本 = 装到最新正式版再回读，回填清单里的 latest 正式版，不留旧号
        const settled = require(inst.id);
        publish(
            {
                ...settled,
                state: 'installed',
                installed_version:
                    version ??
                    (inst.framework_id === 'neobot' ? latestStable(MOCK_NEOBOT_VERSIONS) : null) ??
                    settled.installed_version ??
                    '1.2.5',
            },
            'installed',
        );
    });
    plan.forEach((run, i) => setTimeout(run, 400 + i * 600));
    return taskId;
}
