// Components 页（next）。
//
// 信息架构：组件主导 + 主机分行。每张卡承载一个组件 + 它在所有已知主机
// 上的状态行；卡片宽度自适应，窗口够宽就并排两列，否则单列。
//
// 滚动策略：主体 main 是 overflow-hidden 的 flex 容器，所以这一页内部要自己
// 提供 overflow-y-auto；否则内容超出窗口时会被裁掉而不是出滚动条。
//
// 严守 frontend-layering：仅 import hooks / shared/ui / 自身组件，不直接调
// service / @tauri-apps。

import React, { useCallback, useMemo } from 'react';
import { Box, Loader2, RefreshCw } from 'lucide-react';
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

    // 隐藏"全部 host 都 unsupported"的整张卡。当前 Tauri 模式只有 local
    // 一台 host：LinuxQQ / NoVnc 在本机 Windows 上整张都 unsupported，应当
    // 整张藏掉而不是显示一行"不支持"。
    const visibleView = useMemo(
        () => ({
            framework: view.framework.filter(hasAtLeastOneSupportedHost),
            runtimeDep: view.runtimeDep.filter(hasAtLeastOneSupportedHost),
            selfApp: view.selfApp.filter(hasAtLeastOneSupportedHost),
        }),
        [view],
    );

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

    const allEmpty =
        visibleView.framework.length === 0 &&
        visibleView.runtimeDep.length === 0 &&
        visibleView.selfApp.length === 0;

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            {/* 头部固定，不参与滚动 */}
            <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
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

            {/* 滚动区：组件少时占满高度但不留巨大空白；组件多时正常滚 */}
            <div className="-mr-2 flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto pb-6 pr-2">
                {isLoading && allEmpty && <SectionLoading />}

                <Section
                    title="框架"
                    subtitle="Bot 协议端实现，按需选一种"
                    rows={visibleView.framework}
                    latestVersionFor={latestVersionFor}
                    getProgressFor={getProgressFor}
                    onAction={handleAction}
                    onRefetch={refetch}
                />

                <Section
                    title="运行时依赖"
                    subtitle="框架运行所需的底层环境，通常无需手动操作"
                    rows={visibleView.runtimeDep}
                    latestVersionFor={latestVersionFor}
                    getProgressFor={getProgressFor}
                    onAction={handleAction}
                    onRefetch={refetch}
                />

                <Section
                    title="桌面端"
                    subtitle="本应用的版本与自更新通道"
                    rows={visibleView.selfApp}
                    latestVersionFor={latestVersionFor}
                    getProgressFor={getProgressFor}
                    onAction={handleAction}
                    onRefetch={refetch}
                />
            </div>
        </div>
    );
};

// 卡片是否在当前主机集合里至少有一台 host 支持。把整张都 unsupported 的卡过滤掉。
function hasAtLeastOneSupportedHost(row: ComponentRow): boolean {
    return row.rows.some((r) => r.status.state !== 'unsupported');
}

// ─── Section：标题 + 卡片网格 ───────────────────────────────────────────

interface SectionProps {
    title: string;
    subtitle: string;
    rows: ComponentRow[];
    latestVersionFor: (id: ComponentId) => string | null;
    getProgressFor: ReturnType<typeof useComponentAction>['getProgressFor'];
    onAction: (
        componentId: ComponentId,
        hostId: string,
        payload: { stepKind: StepKind } | { cancelTaskId: string },
    ) => void;
    onRefetch: () => void;
}

const Section: React.FC<SectionProps> = ({
    title,
    subtitle,
    rows,
    latestVersionFor,
    getProgressFor,
    onAction,
    onRefetch,
}) => {
    if (rows.length === 0) return null;
    return (
        <section className="flex flex-col gap-3">
            <div className="flex items-baseline gap-3">
                <h2 className="font-display text-base font-semibold text-text">{title}</h2>
                <p className="text-xs text-text-tertiary">{subtitle}</p>
            </div>
            {/*
              自适应网格：每张卡最少 360px。窗口宽 ≥ 760 自动两列、≥ 1140 三列。
              不再用 [@media(...)]:grid-cols-2 这种死写的断点，靠
              auto-fill + minmax 让 grid 自己根据可用宽度决定列数，永远撑满。
            */}
            <div
                className="grid gap-3"
                style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(min(360px, 100%), 1fr))' }}
            >
                {rows.map((row) => (
                    <ComponentCardWrapper
                        key={row.info.id}
                        row={row}
                        latestVersionFor={latestVersionFor}
                        getProgressFor={getProgressFor}
                        onAction={onAction}
                        onRefetch={onRefetch}
                    />
                ))}
            </div>
        </section>
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

// ─── 子件：占位 / 错误条 ────────────────────────────────────────────

const SectionLoading: React.FC = () => (
    <div className="flex items-center gap-2 rounded-md bg-inset/40 p-6 text-text-tertiary">
        <Loader2 size={16} className="animate-spin" />
        <span className="text-sm">加载中…</span>
    </div>
);

const ErrorBanner: React.FC<{ message: string; onRetry: () => void }> = ({
    message,
    onRetry,
}) => (
    <div className="mb-3 flex shrink-0 items-center justify-between gap-3 rounded-md border border-danger/30 bg-danger-soft px-4 py-3">
        <div className="flex items-center gap-2">
            <Box size={16} className="text-danger" />
            <span className="text-sm text-text">加载组件清单失败：{message}</span>
        </div>
        <Button size="sm" variant="ghost" onClick={onRetry}>
            重试
        </Button>
    </div>
);

export default ComponentsPageNext;
