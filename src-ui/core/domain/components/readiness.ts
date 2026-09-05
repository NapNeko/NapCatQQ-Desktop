// RuntimeReadiness / DependencyPlan 的展示纯函数。依赖图本身在 Rust
// （Component::requirements），前端只消费解析结果，不再自己维护组件链。

import type { BackendType } from '../../ipc/generated/domain/BackendType';
import type { ComponentId } from '../../ipc/generated/domain/ComponentId';
import type { DependencyNode } from '../../ipc/generated/domain/DependencyNode';
import type { DependencyTarget } from '../../ipc/generated/domain/DependencyTarget';
import type { RuntimeReadiness } from '../../ipc/generated/domain/RuntimeReadiness';

/** Bot 的框架组件；与 ncd-runtime framework_component_for 同源字面量 */
export function frameworkComponentFor(backend: BackendType): ComponentId {
    return backend === 'snowluma' ? 'snowluma' : 'napcat';
}

export type ComponentNames = Partial<Record<ComponentId, string>>;

export function targetDisplayName(target: DependencyTarget, names?: ComponentNames): string {
    switch (target.kind) {
        case 'component':
            return names?.[target.id] ?? target.id;
        case 'host_command':
            return target.command;
        case 'host_packages':
            return 'QQ 系统依赖';
    }
}

export function isReady(readiness: RuntimeReadiness): boolean {
    return (
        readiness.root.status.state === 'satisfied' &&
        readiness.plan.nodes.every((n) => n.status.state === 'satisfied')
    );
}

/** 阻断节点，root 在前 */
export function blockingNodes(readiness: RuntimeReadiness): DependencyNode[] {
    return [readiness.root, ...readiness.plan.nodes].filter(
        (n) => n.status.state !== 'satisfied',
    );
}

/** 探测不到的节点；与 Rust describe_not_ready 同规则：不拦启动，只提示 */
export function unknownNodes(readiness: RuntimeReadiness): DependencyNode[] {
    return blockingNodes(readiness).filter((n) => n.status.state === 'unknown');
}

/** 「缺少 A、B；不可用：C（原因）」；可放行（就绪或只剩 unknown）返回 null */
export function describeBlocking(
    readiness: RuntimeReadiness,
    names?: ComponentNames,
): string | null {
    const missing: string[] = [];
    const unusable: string[] = [];
    for (const node of blockingNodes(readiness)) {
        const name = targetDisplayName(node.target, names);
        switch (node.status.state) {
            case 'missing':
            case 'unsupported':
                missing.push(name);
                break;
            case 'unsatisfied':
                unusable.push(`${name}（${node.status.reason}）`);
                break;
            case 'unknown':
            case 'satisfied':
                break;
        }
    }
    const parts: string[] = [];
    if (missing.length) parts.push(`缺少 ${missing.join('、')}`);
    if (unusable.length) parts.push(`不可用：${unusable.join('、')}`);
    return parts.length ? parts.join('；') : null;
}
