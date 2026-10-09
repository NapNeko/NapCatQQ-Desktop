import { useState } from 'react';
import { Button, Checkbox, Select } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import {
    paramsPath,
    records,
    text,
    type PanelObject,
} from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import {
    ObjectView,
    PanelPage,
    PanelSection,
    RecordTable,
    Trend,
    type NeoBotPageProps,
} from './workspaceParts';

export function NeoBotStatisticsTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const [hours, setHours] = useState('24');
    const [bucket, setBucket] = useState('hour');
    const [detail, setDetail] = useState(false);
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
    return (
        <div className="flex flex-col gap-4">
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
            <PanelPage query={messages} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="消息趋势">
                        <Trend data={data} field="count" title="消息数" />
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={latency} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="响应延迟">
                        <Trend data={data} field="ms" title="延迟 ms" />
                        <p className="mt-2 text-xs text-text-secondary">
                            当前 {text(data.current_ms) || '—'} ms · 平均 {text(data.avg_ms) || '—'}{' '}
                            ms · 成功率 {text(data.success_rate) || '—'}
                        </p>
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={usage} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="模型用量">
                        <ObjectView data={data.totals} />
                        {data.available === false && (
                            <p className="mt-3 text-xs text-warning">
                                {text(data.error) || '用量服务不可用'}
                            </p>
                        )}
                        <div className="mt-4">
                            <RecordTable
                                items={records(data.items)}
                                columns={[
                                    ['module', '模块'],
                                    ['provider_name', '供应商'],
                                    ['model_name', '模型'],
                                    ['calls', '调用'],
                                    ['input_tokens', '输入 Token'],
                                    ['output_tokens', '输出 Token'],
                                    ['cost_cny', '金额（元）'],
                                ]}
                            />
                        </div>
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={series} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="用量趋势">
                        <div className="grid gap-5 sm:grid-cols-2">
                            <Trend data={data} field="cost_cny" title="金额（元）" />
                            <Trend data={data} field="input_tokens" title="输入 Token" />
                            <Trend data={data} field="output_tokens" title="输出 Token" />
                            <Trend data={data} field="calls" title="调用次数" />
                        </div>
                        <details className="mt-4 text-xs text-text-secondary">
                            <summary className="cursor-pointer">模型 / 模块排行</summary>
                            <div className="mt-3">
                                <RecordTable
                                    items={[...records(data.models), ...records(data.modules)]}
                                    columns={[
                                        ['model_name', '模型'],
                                        ['module', '模块'],
                                        ['calls', '调用'],
                                        ['cost_cny', '金额（元）'],
                                    ]}
                                />
                            </div>
                        </details>
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={calls} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="API 调用排行">
                        <RecordTable
                            items={records(data.items)}
                            columns={[
                                ['action', '动作'],
                                ['count', '次数'],
                            ]}
                        />
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={users} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="活跃用户">
                        <RecordTable
                            items={records(data.items)}
                            columns={[
                                ['user_id', 'QQ'],
                                ['nickname', '昵称'],
                                ['count', '消息'],
                                ['last_seen', '最近活动'],
                            ]}
                        />
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={recent} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection
                        title="最近调用明细"
                        actions={
                            <Checkbox
                                label="含计费分项"
                                checked={detail}
                                onCheckedChange={(v) => setDetail(v === true)}
                            />
                        }
                    >
                        <RecordTable
                            items={records(data.items)}
                            columns={[
                                ['at', '时间'],
                                ['module', '模块'],
                                ['provider_name', '供应商'],
                                ['model_name', '模型'],
                                ['cost_cny', '金额（元）'],
                                ['cost_source_kind', '计费来源'],
                            ]}
                            onSelect={setSelected}
                        />
                    </PanelSection>
                )}
            </PanelPage>
            {selected && (
                <PanelSection title="调用详情">
                    <ObjectView data={selected} />
                </PanelSection>
            )}
        </div>
    );
}
