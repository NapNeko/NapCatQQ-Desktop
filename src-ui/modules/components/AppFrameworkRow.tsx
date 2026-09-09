// 组件页「应用端」分组里的一张卡：某台主机上某个应用端框架的部署入口。
//
// 应用端按实例安装（每个实例自带 node_modules / .venv），没有主机级「已安装 Karin」状态；
// 卡片展示这台主机上的实例数、运行 / 对接数与运行时依赖是否就绪，主操作是「新建实例」。
// 实例的启停 / 对接 / 日志在「应用端」页。

import React from 'react';
import { ExternalLink, Import, Plus } from 'lucide-react';
import { Button } from '../../shared/ui';
import { ActionMotionIcon, EMPHASIS_MOTION } from '../../shared/ui/motion';
import { useOpenExternal } from '../../hooks/useOpenExternal';
import { ComponentManageCard } from './ComponentEntityCard';
import type { StatusBadgeSpec } from './componentStatusPresentation';
import type { HostInfo, MachineComponentRow } from '../../core/domain/components/types';
import type { AppFrameworkManifest, AppInstance } from '../../core/ipc/types';

/** 运行时依赖在这台主机上的就绪情况；null = 探测中 / 未知 */
export interface RuntimeDepReadiness {
    label: string;
    ready: boolean | null;
}

interface AppFrameworkRowProps {
    manifest: AppFrameworkManifest;
    host: HostInfo;
    /** 这台主机上属于该框架的实例 */
    instances: AppInstance[];
    runtimeDeps: RuntimeDepReadiness[];
    disabled?: boolean;
    onCreate: () => void;
    onImport: () => void;
}

export function hostSupportsAppFramework(host: HostInfo, manifest: AppFrameworkManifest): boolean {
    const placement = host.locality === 'local' ? 'local_native' : 'remote_native';
    return manifest.supported_placements.includes(placement);
}

/** 从这台主机的运行时依赖行里取框架声明的依赖就绪情况 */
export function runtimeDepsFor(
    manifest: AppFrameworkManifest,
    runtimeRows: MachineComponentRow[],
    displayNameOf: (componentId: string) => string,
): RuntimeDepReadiness[] {
    return manifest.runtime_component_ids.map((id) => {
        const row = runtimeRows.find((r) => r.info.id === id);
        const state = row?.status.state;
        const ready =
            state === 'installed' ? true : state === 'not_installed' || state === 'unusable' ? false : null;
        return { label: row?.info.display_name ?? displayNameOf(id), ready };
    });
}

export function appFrameworkStatusBadge(opts: {
    supported: boolean;
    instanceCount: number;
    runningCount: number;
    installing: boolean;
}): StatusBadgeSpec {
    if (!opts.supported) return { tone: 'neutral', label: '不支持' };
    if (opts.installing) return { tone: 'brand', label: '安装中' };
    if (opts.instanceCount === 0) return { tone: 'neutral', label: '未安装', dot: true };
    if (opts.runningCount > 0) {
        return { tone: 'success', label: `运行中 ${opts.runningCount}/${opts.instanceCount}`, dot: true };
    }
    return { tone: 'brand', label: `${opts.instanceCount} 个实例`, dot: true };
}

export const AppFrameworkRow: React.FC<AppFrameworkRowProps> = ({
    manifest,
    host,
    instances,
    runtimeDeps,
    disabled = false,
    onCreate,
    onImport,
}) => {
    const openExternal = useOpenExternal();
    const supported = hostSupportsAppFramework(host, manifest);
    const installing = instances.some((i) => i.state === 'installing');
    const running = instances.filter((i) => i.state === 'running').length;
    const linked = instances.filter((i) => i.link != null).length;

    const badge = appFrameworkStatusBadge({
        supported,
        instanceCount: instances.length,
        runningCount: running,
        installing,
    });

    const metaParts: string[] = [];
    if (instances.length > 0) {
        if (running > 0) metaParts.push(`运行中 ${running}`);
        if (linked > 0) metaParts.push(`已对接 ${linked}`);
    }
    for (const dep of runtimeDeps) {
        metaParts.push(
            dep.ready === true
                ? `${dep.label} 就绪`
                : dep.ready === false
                  ? `${dep.label} 未安装，新建时一并安装`
                  : `${dep.label} 探测中`,
        );
    }

    const footer = supported ? (
        <div className="flex items-center gap-1.5">
            <Button size="sm" variant="secondary" onClick={onImport} disabled={disabled}>
                <ActionMotionIcon icon={Import} size={13} strokeWidth={2.4} />
                导入
            </Button>
            <Button size="sm" variant="primary" onClick={onCreate} disabled={disabled}>
                <ActionMotionIcon icon={Plus} size={13} strokeWidth={2.4} motion={EMPHASIS_MOTION} />
                新建实例
            </Button>
        </div>
    ) : (
        <span className="text-2xs text-text-disabled">—</span>
    );

    return (
        <ComponentManageCard
            accent={installing ? 'brand' : 'none'}
            statusBadge={badge}
            title={manifest.display_name}
            description={manifest.description}
            titleAside={
                manifest.repo_url ? (
                    <button
                        type="button"
                        onClick={() => openExternal(manifest.repo_url!)}
                        className="inline-flex items-center gap-0.5 text-2xs text-text-tertiary transition-colors hover:text-brand"
                    >
                        仓库
                        <ExternalLink size={11} strokeWidth={2} aria-hidden />
                    </button>
                ) : undefined
            }
            meta={
                <p className="truncate text-xs text-text-tertiary" title={metaParts.join(' · ')}>
                    {supported ? metaParts.join(' · ') : '该框架未开放此安装位置'}
                </p>
            }
            footer={footer}
        />
    );
};

export default AppFrameworkRow;
