// 概览页编排：拿数据、算派生，把 7:5 双列摆出来。
// 区块卡在 widgets/（HelloCard、RemoteSummaryCard、NoticeTimelineCard、CoreCardsRow），
// 文案表与格式规则在 core/domain/bootstrap/。

import React, { useMemo } from 'react';
import { usePreferences } from '../../hooks/preferences/usePreferences';
import { useBootstrap } from '../../hooks/bootstrap/useBootstrap';
import { useBackendSettings } from '../../hooks/preferences/useBackendSettings';
import { useBotSnapshots } from '../../hooks/bot/useBotSnapshots';
import { useBotConfigsMap } from '../../hooks/bot/useBotConfigsMap';
import {
    clampPerformanceMonitorIntervalMs,
    PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT,
} from '../../core/domain/performance/performanceSettings';
import { useReleases } from '../../hooks/diagnostics/useReleases';
import { useNoticeEvents } from '../../hooks/events/noticeEventStore';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { buildNotices } from '../../core/domain/events/notice-aggregator';
import { findUpdatesAvailable } from '../../core/domain/release/normalize';
import { computeBotFleetStats, listActionableBots } from '../../core/domain/overview/glance';
import { OverviewCommandColumn, PerformanceChartsSection } from './widgets/OverviewSideColumn';
import { HelloCard } from './widgets/HelloCard';
import { RemoteSummaryCard } from './widgets/RemoteSummaryCard';
import { NoticeTimelineCard } from './widgets/NoticeTimelineCard';
import { CoreCardsRow } from './widgets/CoreCardsRow';
import type { AppRoute } from '../../shared/components/next/Sidebar';

export interface BootstrapPanelNextProps {
    onNavigate?: (route: AppRoute) => void;
}

export const BootstrapPanelNext: React.FC<BootstrapPanelNextProps> = ({ onNavigate }) => {
    const { bootstrap } = useBootstrap();
    const { settings } = useBackendSettings();
    const { snapshot: releases } = useReleases();
    const events = useNoticeEvents();
    const { data: snapshots = [], isSuccess: fleetReady } = useBotSnapshots();
    const configs = useBotConfigsMap(snapshots);

    const monitorEnabled = settings?.performanceMonitorEnabled ?? false;
    const monitorInterval = clampPerformanceMonitorIntervalMs(
        settings?.performanceMonitorIntervalMs ?? PERFORMANCE_MONITOR_INTERVAL_MS_DEFAULT,
    );
    const motionEnabled = usePreferences().motionEnabled;

    const navigate = onNavigate ?? (() => {});

    const notices = useMemo(
        () =>
            buildNotices({
                bootstrap,
                releases,
                recentEvents: events,
            }),
        [bootstrap, releases, events],
    );

    const updates = useMemo(() => {
        if (!bootstrap?.local_versions) return [];
        return findUpdatesAvailable(bootstrap.local_versions, releases);
    }, [bootstrap?.local_versions, releases]);

    const { servers, isLoading: serversLoading } = useServerManager();
    const fleet = useMemo(() => computeBotFleetStats(snapshots), [snapshots]);
    // 与副列「待处置」同一口径（含 6 条上限），两处数字不打架。
    const actionableCount = useMemo(() => listActionableBots(snapshots).length, [snapshots]);

    return (
        <div className="grid min-h-0 flex-1 grid-cols-12 gap-4 pt-8">
            {/* ─── 主列：≥ 1100px 占 7 列 ─── */}
            <div className="col-span-12 flex min-h-0 flex-col gap-4 [@media(min-width:1100px)]:col-span-7">
                <HelloCard
                    fleet={fleet}
                    fleetReady={fleetReady}
                    actionableCount={actionableCount}
                    serverCount={servers.length}
                    onNavigate={navigate}
                />
                <RemoteSummaryCard
                    servers={servers}
                    isLoading={serversLoading}
                    onNavigate={navigate}
                />
                <NoticeTimelineCard
                    notices={notices}
                    onNavigate={navigate}
                    className="min-h-0 flex-1"
                />
            </div>

            {/* ─── 副列：≥ 1100px 占 5 列 ─── */}
            <div className="col-span-12 flex min-h-0 flex-col gap-4 [@media(min-width:1100px)]:col-span-5">
                <CoreCardsRow
                    napcatVersion={bootstrap?.local_versions.napcat ?? null}
                    snowlumaVersion={bootstrap?.local_versions.snowluma ?? null}
                    updates={updates}
                    onNavigate={navigate}
                />
                {monitorEnabled ? (
                    <PerformanceChartsSection
                        sampleIntervalMs={monitorInterval}
                        motionEnabled={motionEnabled}
                    />
                ) : (
                    <OverviewCommandColumn
                        snapshots={snapshots}
                        configs={configs}
                        onNavigate={navigate}
                    />
                )}
            </div>
        </div>
    );
};

export default BootstrapPanelNext;
