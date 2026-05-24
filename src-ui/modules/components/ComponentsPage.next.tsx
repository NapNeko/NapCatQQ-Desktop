// Components 页（next）。
//
// 信息架构：组件主导 + 主机分行（参见 v1 设计）。
//   ┌─ 框架 ─────────────────────┐
//   │ ┌─ NapCat ─────────────┐  │
//   │ │ 本机:        v4.18.1  │  │
//   │ │ remote-prod: v4.20.0  │  │
//   │ │ remote-dev:  未安装   │  │
//   │ └───────────────────────┘  │
//   │ ┌─ SnowLuma ────────────┐  │
//   │ └───────────────────────┘  │
//   └────────────────────────────┘
//   ┌─ 运行时依赖 ───────────────┐
//   │ Node.js / LinuxQQ / noVNC  │
//   └────────────────────────────┘
//
// 严守 frontend-layering：仅 import hooks / shared/ui / 自身组件，不直接调
// service / @tauri-apps。

import React, { useCallback } from 'react';
import { Box, Loader2, Package, RefreshCw } from 'lucide-react';
import { Button } from '../../shared/ui';
import { useComponents } from '../../hooks/components/useComponents';
import { useComponentAction } from '../../hooks/components/useComponentAction';
import { useReleases } from '../../hooks/diagnostics/useReleases';
import { ComponentCard } from './ComponentCard';
import type { ComponentId, StepKind } from '../../core/ipc/types';
import type { ComponentRow } from '../../core/domain/components/types';

export const ComponentsPageNext: React.FC = () => {
    const { view, isLoading, error, refetch } = useComponents();
    const { startAction, cancelAction, getProgressFor } = useComponentAction();
    const { snapshot: releases } = useReleases();

    const latestVersionFor = useCallback(
        (id: ComponentId): string | null => {
            // 目前只有 napcat / snowluma / desktop_self 在 ReleaseSnapshot 里有
            // 远端版本，其它组件返回 null（UI 不显示"有更新"角标）。
            switch (id) {
                case 'napcat':
                    return releases.napcat?.version ?? null;
                case 'snowluma':
                    return releases.snowluma?.version ?? null;
                case 'desktop_self':
                    return releases.desktop?.version ?? null;
                default:
                    return null;
            }
        },
        [releases],
    );

    const handleAction = useCallback(
        async (
            componentId: ComponentId,
            hostId: string,
            payload: { stepKind: StepKind } | { cancelTaskId: string },
        ) => {
            try {
                if ('cancelTaskId' in payload) {
                    await cancelAction(payload.cancelTaskId);
                    return;
                }
                await startAction(componentId, hostId, payload.stepKind);
                // 操作成功后稍后再 refetch detect（500ms 等后端同步）
                setTimeout(refetch, 500);
            } catch (err) {
                console.error('[ComponentsPage] action failed:', err);
            }
        },
        [startAction, cancelAction, refetch],
    );

    return (
        <div className="flex min-h-0 flex-1 flex-col gap-4 pt-2">
            <header className="flex shrink-0 items-end justify-between">
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">
                        components
                    </p>
                    <h1 className="font-display text-xl font-semibold text-text">
                        组件管理
                    </h1>
                    <p className="mt-1 text-sm text-text-secondary">
                        在本机或远端主机上安装、更新、卸载 Bot 框架及其运行时依赖。
                    </p>
                </div>
                <Button
                    size="sm"
                    variant="secondary"
                    onClick={refetch}
                    disabled={isLoading}
                >
                    <RefreshCw size={14} className={isLoading ? 'animate-spin' : undefined} />
                    刷新
                </Button>
            </header>

            {error && <ErrorBanner message={error.message} onRetry={refetch} />}

            {/* Framework section */}
            <SectionHeader title="框架" subtitle="用户主动选择安装的 Bot 框架" />
            {isLoading && view.framework.length === 0 ? (
                <SectionLoading />
            ) : view.framework.length === 0 ? (
                <SectionEmpty message="暂无可用框架" />
            ) : (
                <div className="grid grid-cols-1 gap-4 [@media(min-width:1100px)]:grid-cols-2">
                    {view.framework.map((row) => (
                        <ComponentCardWrapper
                            key={row.info.id}
                            row={row}
                            latestVersionFor={latestVersionFor}
                            getProgressFor={getProgressFor}
                            onAction={handleAction}
                            onRefetch={refetch}
                        />
                    ))}
                </div>
            )}

            {/* Runtime deps section */}
            {view.runtimeDep.length > 0 && (
                <>
                    <SectionHeader
                        title="运行时依赖"
                        subtitle="框架运行所需的底层环境，通常无需手动操作"
                    />
                    <div className="grid grid-cols-1 gap-4 [@media(min-width:1100px)]:grid-cols-2">
                        {view.runtimeDep.map((row) => (
                            <ComponentCardWrapper
                                key={row.info.id}
                                row={row}
                                latestVersionFor={latestVersionFor}
                                getProgressFor={getProgressFor}
                                onAction={handleAction}
                                onRefetch={refetch}
                            />
                        ))}
                    </div>
                </>
            )}

            {/* SelfApp section（如果有） */}
            {view.selfApp.length > 0 && (
                <>
                    <SectionHeader
                        title="桌面端"
                        subtitle="本应用的版本与自更新通道"
                    />
                    <div className="grid grid-cols-1 gap-4">
                        {view.selfApp.map((row) => (
                            <ComponentCardWrapper
                                key={row.info.id}
                                row={row}
                                latestVersionFor={latestVersionFor}
                                getProgressFor={getProgressFor}
                                onAction={handleAction}
                                onRefetch={refetch}
                            />
                        ))}
                    </div>
                </>
            )}
        </div>
    );
};

// ─── Wrapper：把 hooks 提供的派生函数喂给 ComponentCard ───────────────

const ComponentCardWrapper: React.FC<{
    row: ComponentRow;
    latestVersionFor: (id: ComponentId) => string | null;
    getProgressFor: ReturnType<typeof useComponentAction>['getProgressFor'];
    onAction: (
        componentId: ComponentId,
        hostId: string,
        payload: { stepKind: StepKind } | { cancelTaskId: string },
    ) => void;
    onRefetch: () => void;
}> = ({ row, latestVersionFor, getProgressFor, onAction, onRefetch }) => {
    return (
        <ComponentCard
            data={row}
            latestRemoteVersion={latestVersionFor(row.info.id)}
            getProgress={(hostId) => getProgressFor(row.info.id, hostId)}
            onAction={(hostId, payload) => onAction(row.info.id, hostId, payload)}
            onRetryDetect={() => onRefetch()}
        />
    );
};

// ─── 子件：section header / 占位 ────────────────────────────────────────

const SectionHeader: React.FC<{ title: string; subtitle: string }> = ({
    title,
    subtitle,
}) => (
    <div className="flex shrink-0 items-baseline gap-3 pt-2">
        <h2 className="font-display text-[14px] font-semibold text-text">
            {title}
        </h2>
        <p className="text-[12px] text-text-tertiary">{subtitle}</p>
    </div>
);

const SectionLoading: React.FC = () => (
    <div className="flex items-center gap-2 rounded-md bg-inset/40 p-6 text-text-tertiary">
        <Loader2 size={16} className="animate-spin" />
        <span className="text-[13px]">加载中…</span>
    </div>
);

const SectionEmpty: React.FC<{ message: string }> = ({ message }) => (
    <div className="flex items-center gap-2 rounded-md p-6 text-text-tertiary">
        <Package size={16} />
        <span className="text-[13px]">{message}</span>
    </div>
);

const ErrorBanner: React.FC<{ message: string; onRetry: () => void }> = ({
    message,
    onRetry,
}) => (
    <div className="flex shrink-0 items-center justify-between gap-3 rounded-md border border-danger/30 bg-danger-soft px-4 py-3">
        <div className="flex items-center gap-2">
            <Box size={16} className="text-danger" />
            <span className="text-[13px] text-text">加载组件清单失败：{message}</span>
        </div>
        <Button size="sm" variant="ghost" onClick={onRetry}>
            重试
        </Button>
    </div>
);

export default ComponentsPageNext;
