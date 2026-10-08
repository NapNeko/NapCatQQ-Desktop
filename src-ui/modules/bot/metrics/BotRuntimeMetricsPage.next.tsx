// Bot 运行时指标全页：与配置 / 日志同级。
// 单页自适应仪表盘：总览 / 系统资源 / 节点流量同屏，页面本身不滚动。
// 本文件只做编排：卡片子件在同目录 metrics*Parts / metricsRangePanel，
// 展示派生算术在 core/domain/bot/runtime-metrics-display。

import { useEffect, useMemo, useState } from 'react';
import { AlertTriangle, ArrowLeft, CircleHelp, RefreshCw } from 'lucide-react';
import {
    Badge,
    Button,
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { useBotRuntimeMetrics } from '../../../hooks/bot/useBotRuntimeMetrics';
import { useBotRuntimeMetricsHistory } from '../../../hooks/bot/useBotRuntimeMetricsHistory';
import { useBotConfigsMap } from '../../../hooks/bot/useBotConfigsMap';
import { useBotSnapshots } from '../../../hooks/bot/useBotSnapshots';
import {
    formatBytes,
    formatCollectedAgo,
    formatCompactCount,
    isMetricsHistoryRangeAvailable,
    probeHealthLabel,
    rssBytesOf,
    sumNodeTotals,
    type MetricsHistoryWindow,
} from '../../../core/domain/bot/runtime-metrics-settings';
import {
    activeNodesCountOf,
    probeBadgeTone,
} from '../../../core/domain/bot/runtime-metrics-display';
import { isRuntimeTargetLocal } from '../../../core/domain/bot/runtime-target';
import { cn } from '../../../shared/utils/cn';
import type { HistorySeriesKey } from './BotRuntimeMetricsHistoryChart';
import { KpiTile, Panel } from './metricsPanelParts';
import { NodeRows, SystemResourceCard } from './metricsResourceParts';
import { TrendConfigMenu, TrendChartCard } from './metricsTrendParts';
import { TrendRangePanel } from './metricsRangePanel';

export interface BotRuntimeMetricsPageNextProps {
    botId: string;
    onBack: () => void;
}

export function BotRuntimeMetricsPageNext({ botId, onBack }: BotRuntimeMetricsPageNextProps) {
    const { data: bots = [] } = useBotSnapshots();
    const configByBot = useBotConfigsMap(bots);
    const config = configByBot[botId] ?? null;
    const displayName =
        config?.bot.name && config.bot.name.trim().length > 0 ? config.bot.name.trim() : botId;
    const flavor = config?.bot.backend_type ?? null;
    const isRemote =
        config?.bot.runtime_target != null && !isRuntimeTargetLocal(config.bot.runtime_target);

    const { enabled, metrics, loading, retentionDays, refresh } = useBotRuntimeMetrics(botId, {
        liveDetail: true,
    });

    const [historyWindow, setHistoryWindow] = useState<MetricsHistoryWindow>({
        mode: 'preset',
        range: '1h',
    });
    const [series, setSeries] = useState<HistorySeriesKey>('rss');
    /** false = 从 0 起（默认）；true = 贴合数据区间看小波动 */
    const [fitData, setFitData] = useState(false);
    const [showDots, setShowDots] = useState(false);
    const history = useBotRuntimeMetricsHistory(botId, historyWindow, retentionDays, true);

    useEffect(() => {
        if (historyWindow.mode !== 'preset') return;
        if (!isMetricsHistoryRangeAvailable(historyWindow.range, retentionDays)) {
            setHistoryWindow({ mode: 'preset', range: '1h' });
        }
    }, [historyWindow, retentionDays]);

    const totals = sumNodeTotals(metrics?.nodes);
    const rss = rssBytesOf(metrics);
    const heap = metrics?.memory?.heap_used_bytes;

    const probe = metrics?.probe ?? 'not_injected';
    const showInjectHint = probe === 'not_injected' || probe === 'error';
    const activeNodes = useMemo(() => activeNodesCountOf(metrics?.nodes), [metrics?.nodes]);

    const onRefreshAll = () => {
        refresh();
        history.refresh();
    };

    const statusBanner = !enabled ? (
        <div className="rounded-sm bg-inset/50 px-3 py-2 text-2xs leading-relaxed text-text-secondary ring-1 ring-border-subtle">
            实例指标未启用。请到「设置 · 监控」打开并保存，然后重启该 Bot。
        </div>
    ) : showInjectHint ? (
        <div
            className={cn(
                'rounded-sm px-3 py-2 text-2xs leading-relaxed ring-1',
                probe === 'error'
                    ? 'bg-danger-soft/35 text-text-secondary ring-danger/20'
                    : 'bg-warning-soft/35 text-text-secondary ring-warning/20',
            )}
        >
            <div className="flex gap-2">
                <AlertTriangle
                    aria-hidden
                    size={14}
                    className={cn(
                        'mt-0.5 shrink-0',
                        probe === 'error' ? 'text-danger' : 'text-warning',
                    )}
                />
                <div className="min-w-0">
                    <p className="font-medium text-text">
                        {probe === 'error' ? '暂时无法读取运行时指标' : '探针尚未载入此 Bot'}
                    </p>
                    <p className="mt-0.5 text-text-tertiary">
                        {probe === 'error'
                            ? isRemote
                                ? '检查远端连接、ncd-watch 同步与探针注入。'
                                : '可尝试重启实例；持续失败请查 Desktop 日志。'
                            : isRemote
                              ? '开启指标并同步 ncd-watch 后，在该机重启 Bot。'
                              : '设置 · 监控启用并保存后，重启该实例。'}
                    </p>
                </div>
            </div>
        </div>
    ) : null;

    const overviewHelpText = isRemote
        ? '远端历史由同机 ncd-watch 续写，Desktop 退出后不会中断。'
        : '本机历史由 Desktop 在线时写入；关闭应用后暂停采样。';

    const overviewAside = (
        <TooltipProvider delayDuration={120}>
            <Tooltip>
                <TooltipTrigger asChild>
                    <button
                        type="button"
                        title={overviewHelpText}
                        className="inline-flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40"
                        aria-label="指标说明"
                    >
                        <CircleHelp aria-hidden size={15} strokeWidth={2.1} />
                    </button>
                </TooltipTrigger>
                <TooltipContent
                    side="bottom"
                    sideOffset={8}
                    className="max-w-[17rem] text-left font-normal leading-relaxed"
                >
                    {overviewHelpText}
                </TooltipContent>
            </Tooltip>
        </TooltipProvider>
    );

    return (
        <div className="flex h-full min-h-0 w-full flex-col overflow-hidden">
            <header className="flex shrink-0 items-center justify-between gap-3 border-b border-border-subtle py-2.5">
                <div className="flex min-w-0 items-center gap-2.5">
                    <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回列表">
                        <ActionMotionIcon icon={ArrowLeft} size={16} />
                    </Button>
                    <div className="min-w-0">
                        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                            <h1 className="truncate font-display text-[15px] font-semibold text-text">
                                运行时指标
                            </h1>
                            <Badge tone={probeBadgeTone(probe)} appearance="soft">
                                {probeHealthLabel(probe)}
                            </Badge>
                            {isRemote ? (
                                <Badge tone="info" appearance="soft">
                                    远端
                                </Badge>
                            ) : null}
                            {!enabled ? (
                                <Badge tone="neutral" appearance="soft">
                                    未启用
                                </Badge>
                            ) : null}
                        </div>
                        <p className="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-1.5 text-2xs text-text-tertiary">
                            <span className="truncate font-medium text-text-secondary">
                                {displayName}
                            </span>
                            <span aria-hidden className="text-border">
                                ·
                            </span>
                            <span className="font-mono tabular-nums">QQ {botId}</span>
                            {flavor ? (
                                <>
                                    <span aria-hidden className="text-border">
                                        ·
                                    </span>
                                    <span>{flavor}</span>
                                </>
                            ) : null}
                            <span aria-hidden className="text-border">
                                ·
                            </span>
                            <span className="tabular-nums">
                                {loading && !metrics
                                    ? '加载中…'
                                    : formatCollectedAgo(metrics?.collected_at_ms)}
                            </span>
                            {metrics ? (
                                <>
                                    <span aria-hidden className="text-border">
                                        ·
                                    </span>
                                    <span>
                                        {metrics.source}
                                        {isRemote ? ' · 远端' : ' · 本机'}
                                    </span>
                                </>
                            ) : null}
                        </p>
                    </div>
                </div>
                <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={loading}
                    onClick={onRefreshAll}
                >
                    <RefreshCw
                        aria-hidden
                        size={14}
                        className={cn('mr-1.5', loading && 'animate-spin')}
                    />
                    {loading ? '刷新中…' : '刷新'}
                </Button>
            </header>

            <div className="grid min-h-0 flex-1 grid-cols-1 gap-2.5 overflow-hidden pt-2.5 lg:grid-cols-12 lg:grid-rows-2">
                <div className="grid min-h-0 grid-cols-1 gap-2.5 lg:col-span-7 lg:row-span-2 lg:grid-rows-2">
                    <div className="grid min-h-0 grid-cols-1 gap-2.5 sm:grid-cols-2">
                        <Panel title="总览" className="min-h-0" aside={overviewAside}>
                            <div className="flex h-full min-h-0 flex-col gap-2">
                                {statusBanner}
                                <div className="@container min-h-0 flex-1">
                                    <div className="grid h-full min-h-0 grid-cols-2 grid-rows-2 gap-2">
                                        <KpiTile
                                            label="内存 RSS"
                                            value={formatBytes(rss)}
                                            hint={
                                                heap != null
                                                    ? `堆 ${formatBytes(Number(heap))}`
                                                    : undefined
                                            }
                                        />
                                        <KpiTile
                                            label="活跃节点"
                                            value={`${activeNodes} / ${metrics?.nodes.length ?? 0}`}
                                            tone={activeNodes > 0 ? 'success' : 'neutral'}
                                            hint={
                                                (metrics?.nodes.length ?? 0) === 0
                                                    ? '暂无节点'
                                                    : activeNodes > 0
                                                      ? undefined
                                                      : '暂无活动'
                                            }
                                        />
                                        <KpiTile
                                            label="事件 出 / 入"
                                            value={`${formatCompactCount(totals.eventsOut)} / ${formatCompactCount(totals.actionsIn)}`}
                                            hint={`${formatBytes(totals.bytesOut)} / ${formatBytes(totals.bytesIn)}`}
                                        />
                                        <KpiTile
                                            label="累计错误"
                                            value={formatCompactCount(totals.errors)}
                                            tone={totals.errors > 0 ? 'danger' : 'neutral'}
                                            hint={totals.errors > 0 ? '需关注节点表' : undefined}
                                        />
                                    </div>
                                </div>
                            </div>
                        </Panel>

                        <SystemResourceCard metrics={metrics} rss={rss} isRemote={isRemote} />
                    </div>

                    <Panel
                        title="节点与出入流量"
                        className="min-h-0"
                        aside={
                            <span className="font-mono text-[10px] tabular-nums text-text-tertiary">
                                出 {formatBytes(totals.bytesOut)} · 入 {formatBytes(totals.bytesIn)}
                            </span>
                        }
                    >
                        <NodeRows nodes={metrics?.nodes ?? []} />
                    </Panel>
                </div>

                <TrendChartCard
                    aside={
                        <div className="flex items-center gap-1">
                            <TrendRangePanel
                                window={historyWindow}
                                onChange={setHistoryWindow}
                                retentionDays={retentionDays}
                            />
                            <TrendConfigMenu
                                series={series}
                                onSeriesChange={setSeries}
                                fitData={fitData}
                                onFitDataChange={setFitData}
                                showDots={showDots}
                                onShowDotsChange={setShowDots}
                            />
                        </div>
                    }
                    totals={totals}
                    points={history.points}
                    loading={history.loading}
                    error={history.error}
                    onRetry={history.refresh}
                    series={series}
                    fitData={fitData}
                    showDots={showDots}
                />
            </div>
        </div>
    );
}

export default BotRuntimeMetricsPageNext;
