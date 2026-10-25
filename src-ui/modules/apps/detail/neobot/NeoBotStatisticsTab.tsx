import { useState } from 'react';
import {
    Button,
    Checkbox,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    Select,
} from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import { panelPageGate, panelQueryState } from '../../../../core/domain/apps/neobotPanelState';
import {
    paramsPath,
    record,
    records,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import {
    FieldList,
    InlineNote,
    Metric,
    MetricGrid,
    PanelQuerySection,
    PanelSection,
    PanelTable,
    Trend,
    type NeoBotPageProps,
} from './workspaceParts';
import { PanelDisclosure } from './PanelDisclosure';
import { PanelGateView } from './PanelStateView';

const TREND_TITLES: Record<string, string> = {
    messages: '消息数',
    latency: '响应延迟 ms',
    cost_cny: '金额（元）',
    calls: '调用次数',
    input_tokens: '输入 Token',
    output_tokens: '输出 Token',
};

export function NeoBotStatisticsTab({ instanceId }: NeoBotPageProps) {
    const [hours, setHours] = useState('24');
    const [bucket, setBucket] = useState('hour');
    const [detail, setDetail] = useState(false);
    const [trendMetric, setTrendMetric] = useState('cost_cny');
    const [selected, setSelected] = useState<PanelObject | null>(null);
    const messagesPath = paramsPath('/api/series/messages', {
        days: Math.max(1, Math.ceil(Number(hours) / 24)),
    });
    const usagePath = paramsPath('/api/stats/usage', { hours });
    const seriesPath = paramsPath('/api/series/usage', { hours, bucket });
    const recordsPath = paramsPath('/api/stats/usage/records', {
        hours,
        limit: 100,
        detail: detail ? 1 : 0,
    });
    const messages = usePanelJson(instanceId, messagesPath, messagesPath, asRecord);
    const latency = usePanelJson(instanceId, 'latency', '/api/series/latency', asRecord);
    const calls = usePanelJson(instanceId, 'apiCalls', '/api/stats/api-calls?limit=100', asRecord);
    const users = usePanelJson(
        instanceId,
        'activeUsers',
        '/api/stats/active-users?limit=100',
        asRecord,
    );
    const usage = usePanelJson(instanceId, usagePath, usagePath, asRecord);
    const series = usePanelJson(instanceId, seriesPath, seriesPath, asRecord);
    const recent = usePanelJson(instanceId, recordsPath, recordsPath, asRecord);
    const refresh = () => {
        for (const q of [messages, latency, calls, users, usage, series, recent]) void q.refetch();
    };
    const gate = panelPageGate(
        [messages, latency, calls, users, usage, series, recent].map(panelQueryState),
    );
    const trendQuery =
        trendMetric === 'messages' ? messages : trendMetric === 'latency' ? latency : series;
    const trendTitle = TREND_TITLES[trendMetric] ?? '用量';
    if (gate) return <PanelGateView gate={gate} onRetry={refresh} />;
    return (
        <div className="flex flex-col gap-4">
            <PanelSection
                title="统计范围"
                meta="范围用于消息与模型用量；粒度仅用于用量趋势。延迟、API 与活跃用户按面板当前窗口统计。"
            >
                <div className="flex flex-wrap items-end gap-3">
                    <Select
                        label="统计范围"
                        value={hours}
                        items={[
                            { value: '24', label: '24 小时' },
                            { value: '168', label: '7 天' },
                            { value: '720', label: '30 天' },
                        ]}
                        onValueChange={(h) => {
                            setHours(h);
                            setSelected(null);
                        }}
                    />
                    <Select
                        label="用量粒度"
                        value={bucket}
                        items={[
                            { value: 'hour', label: '每小时' },
                            { value: 'day', label: '每天' },
                        ]}
                        onValueChange={setBucket}
                    />
                    <Button size="sm" variant="secondary" onClick={refresh}>
                        刷新统计
                    </Button>
                </div>
            </PanelSection>
            <PanelQuerySection
                title="趋势"
                query={trendQuery}
                actions={
                    <Select
                        label="趋势指标"
                        value={trendMetric}
                        onValueChange={setTrendMetric}
                        items={[
                            { value: 'cost_cny', label: '金额' },
                            { value: 'calls', label: '调用次数' },
                            { value: 'input_tokens', label: '输入 Token' },
                            { value: 'output_tokens', label: '输出 Token' },
                            { value: 'messages', label: '消息数' },
                            { value: 'latency', label: '响应延迟' },
                        ]}
                    />
                }
            >
                {(data) => (
                    <div className="flex flex-col gap-3">
                        <Trend
                            data={data}
                            field={
                                trendMetric === 'messages'
                                    ? 'count'
                                    : trendMetric === 'latency'
                                      ? 'ms'
                                      : trendMetric
                            }
                            title={trendTitle}
                        />
                        {trendMetric === 'latency' && (
                            <MetricGrid>
                                <Metric
                                    label="当前延迟"
                                    value={text(data.current_ms) || '—'}
                                    unit="ms"
                                />
                                <Metric
                                    label="平均延迟"
                                    value={text(data.avg_ms) || '—'}
                                    unit="ms"
                                />
                                <Metric label="成功率" value={text(data.success_rate) || '—'} />
                            </MetricGrid>
                        )}
                    </div>
                )}
            </PanelQuerySection>
            <PanelQuerySection title="模型用量" query={usage}>
                {(data) => (
                    <div className="flex flex-col gap-3">
                        <MetricGrid cols={4}>
                            <Metric
                                label="调用次数"
                                value={text(record(data.totals).calls) || '—'}
                            />
                            <Metric
                                label="输入 Token"
                                value={text(record(data.totals).input_tokens) || '—'}
                            />
                            <Metric
                                label="输出 Token"
                                value={text(record(data.totals).output_tokens) || '—'}
                            />
                            <Metric
                                label="金额"
                                value={text(record(data.totals).cost_cny) || '—'}
                                unit="元"
                                tone="brand"
                            />
                        </MetricGrid>
                        {data.available === false && (
                            <InlineNote tone="warning">
                                {text(data.error) || '用量服务不可用'}
                            </InlineNote>
                        )}
                        <PanelTable
                            rows={records(data.items)}
                            empty="这个范围内还没有模型调用，可切换更长的范围"
                            columns={[
                                { key: 'module', label: '模块' },
                                { key: 'provider_name', label: '供应商' },
                                { key: 'model_name', label: '模型' },
                                { key: 'calls', label: '调用', align: 'right' },
                                { key: 'input_tokens', label: '输入 Token', align: 'right' },
                                { key: 'output_tokens', label: '输出 Token', align: 'right' },
                                { key: 'cost_cny', label: '金额（元）', align: 'right' },
                            ]}
                        />
                    </div>
                )}
            </PanelQuerySection>
            <PanelQuerySection title="模型与模块排行" query={series}>
                {(data) => (
                    <PanelDisclosure label="展开排行">
                        <PanelTable
                            rows={[...records(data.models), ...records(data.modules)]}
                            empty="这个范围内还没有模型或模块排行"
                            columns={[
                                { key: 'model_name', label: '模型' },
                                { key: 'module', label: '模块' },
                                { key: 'calls', label: '调用', align: 'right' },
                                { key: 'cost_cny', label: '金额（元）', align: 'right' },
                            ]}
                        />
                    </PanelDisclosure>
                )}
            </PanelQuerySection>
            <div className="grid gap-4 xl:grid-cols-2">
                <PanelQuerySection title="API 调用排行" query={calls}>
                    {(data) => (
                        <PanelTable
                            rows={records(data.items)}
                            empty="还没有 API 调用记录"
                            columns={[
                                { key: 'action', label: '动作' },
                                { key: 'count', label: '次数', align: 'right' },
                            ]}
                        />
                    )}
                </PanelQuerySection>
                <PanelQuerySection title="活跃用户" query={users}>
                    {(data) => (
                        <PanelTable
                            rows={records(data.items)}
                            empty="还没有用户活动记录"
                            columns={[
                                { key: 'user_id', label: 'QQ', mono: true },
                                { key: 'nickname', label: '昵称' },
                                { key: 'count', label: '消息', align: 'right' },
                                { key: 'last_seen', label: '最近活动' },
                            ]}
                        />
                    )}
                </PanelQuerySection>
            </div>
            <PanelQuerySection
                query={recent}
                title="最近调用明细"
                actions={
                    <Checkbox
                        label="含计费分项"
                        checked={detail}
                        onCheckedChange={(v) => setDetail(v === true)}
                    />
                }
            >
                {(data) => (
                    <PanelTable
                        rows={records(data.items)}
                        empty="这个范围内还没有调用明细"
                        columns={[
                            { key: 'at', label: '时间' },
                            { key: 'module', label: '模块' },
                            { key: 'provider_name', label: '供应商' },
                            { key: 'model_name', label: '模型' },
                            { key: 'cost_cny', label: '金额（元）', align: 'right' },
                            { key: 'cost_source_kind', label: '计费来源' },
                            {
                                key: 'detail',
                                label: '详情',
                                render: (row) => (
                                    <Button
                                        size="sm"
                                        variant="ghost"
                                        onClick={() => setSelected(row)}
                                    >
                                        查看
                                    </Button>
                                ),
                            },
                        ]}
                    />
                )}
            </PanelQuerySection>
            <Dialog
                open={selected !== null}
                onOpenChange={(open) => {
                    if (!open) setSelected(null);
                }}
            >
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>调用详情</DialogTitle>
                    </DialogHeader>
                    {selected && (
                        <FieldList
                            data={selected}
                            labels={{
                                at: '时间',
                                module: '模块',
                                provider_name: '供应商',
                                model_name: '模型',
                                cost_cny: '金额（元）',
                                cost_source_kind: '计费来源',
                                input_tokens: '输入 Token',
                                output_tokens: '输出 Token',
                            }}
                        />
                    )}
                </DialogContent>
            </Dialog>
        </div>
    );
}
