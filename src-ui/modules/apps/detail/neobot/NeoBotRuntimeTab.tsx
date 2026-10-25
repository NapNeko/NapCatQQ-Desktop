import { Badge, Button, Checkbox } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import { panelPageGate, panelQueryState } from '../../../../core/domain/apps/neobotPanelState';
import { records, text, type PanelObject } from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import {
    ConfirmAction,
    FieldList,
    InlineNote,
    Metric,
    MetricGrid,
    PanelQuerySection,
    PanelSection,
    PanelTable,
    type NeoBotPageProps,
} from './workspaceParts';
import { PanelGateView, PanelSectionState } from './PanelStateView';
import { PanelDisclosure } from './PanelDisclosure';

function memoryPercent(data: PanelObject) {
    if (typeof data.mem_percent === 'number' && Number.isFinite(data.mem_percent))
        return data.mem_percent;
    const used = Number(data.mem_used_mb);
    const total = Number(data.mem_total_mb);
    return Number.isFinite(used) && total > 0 && Number.isFinite(total)
        ? Math.round((used / total) * 1000) / 10
        : '—';
}

export function NeoBotRuntimeTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const power = usePanelJson(instanceId, 'power', '/api/admin/power', asRecord, true, 3000);
    const system = usePanelJson(instanceId, 'system', '/api/system', asRecord, true, 10000);
    const services = usePanelJson(instanceId, 'services', '/api/services', asRecord);
    const tasks = usePanelJson(instanceId, 'tasks', '/api/tasks', asRecord, true, 10000);
    const bot = usePanelJson(instanceId, 'botDetail', '/api/bot/detail', asRecord);
    const bots = usePanelJson(instanceId, 'bots', '/api/bots', (raw) =>
        Array.isArray(raw) ? { items: raw } : asRecord(raw),
    );
    const action = useNeoBotAction(instanceId);
    const gate = panelPageGate([power, system, services, tasks, bot, bots].map(panelQueryState));
    const refresh = () => {
        for (const query of [power, system, services, tasks, bot, bots]) void query.refetch();
    };
    if (gate) return <PanelGateView gate={gate} onRetry={refresh} />;
    return (
        <div className="flex flex-col gap-4">
            <PanelQuerySection
                title="运行状态"
                meta="每 3 秒刷新"
                query={power}
                actions={
                    <Button size="sm" variant="ghost" onClick={refresh}>
                        刷新全部
                    </Button>
                }
            >
                {(data) => {
                    const disabled =
                        action.isPending || data.transition === true || data.available === false;
                    return (
                        <div className="flex flex-col gap-4">
                            <div className="flex flex-wrap items-center gap-3">
                                <Badge
                                    tone={
                                        data.available === false
                                            ? 'danger'
                                            : data.transition === true || data.standby === true
                                              ? 'warning'
                                              : 'success'
                                    }
                                >
                                    {data.available === false
                                        ? '运行时不可用'
                                        : data.transition === true
                                          ? '正在切换'
                                          : data.standby === true
                                            ? '待机中'
                                            : '运行中'}
                                </Badge>
                                {text(data.reason) && (
                                    <span className="text-xs text-text-secondary">
                                        {text(data.reason)}
                                    </span>
                                )}
                            </div>
                            <div className="flex flex-wrap gap-2">
                                <ConfirmAction
                                    label="进入待机"
                                    description="停止回复与记忆处理，保留面板和核心服务。"
                                    disabled={disabled || data.standby === true}
                                    onConfirm={() =>
                                        void action.run({
                                            path: '/api/admin/standby',
                                            body: { reason: 'desktop' },
                                        })
                                    }
                                />
                                <Button
                                    size="sm"
                                    variant="primary"
                                    disabled={disabled || data.standby !== true}
                                    onClick={() =>
                                        void action.run({
                                            path: '/api/admin/resume',
                                            body: { reason: 'desktop' },
                                        })
                                    }
                                >
                                    恢复运行
                                </Button>
                                <ConfirmAction
                                    label="软重启运行"
                                    description="重新构建 Bot 运行时，处理中的回复会中断。面板进程继续保留。"
                                    disabled={disabled}
                                    onConfirm={() =>
                                        void action.run({
                                            path: '/api/admin/reboot',
                                            body: { reason: 'desktop' },
                                        })
                                    }
                                />
                                <ConfirmAction
                                    label="重启进程"
                                    description="重启 NeoBot，面板会短暂断开，代码与启动期配置随后生效。"
                                    disabled={action.isPending || data.transition === true}
                                    onConfirm={() =>
                                        void action.run({ path: '/api/admin/restart' })
                                    }
                                />
                            </div>
                            <div className="flex flex-col gap-1.5">
                                <Checkbox
                                    label="待机时保持 OneBot 连接"
                                    checked={data.connect_onebot === true}
                                    disabled={disabled}
                                    onCheckedChange={(enabled) =>
                                        void action.run({
                                            path: '/api/admin/standby/onebot',
                                            body: { enabled: enabled === true },
                                        })
                                    }
                                />
                                <p className="text-2xs text-text-tertiary">
                                    关闭后，待机期间会断开协议连接。
                                </p>
                            </div>
                            <PanelDisclosure label="运行详情">
                                <FieldList
                                    data={data}
                                    hide={[
                                        'standby',
                                        'transition',
                                        'available',
                                        'reason',
                                        'connect_onebot',
                                    ]}
                                    columns={2}
                                />
                            </PanelDisclosure>
                        </div>
                    );
                }}
            </PanelQuerySection>
            <PanelQuerySection title="系统资源" meta="每 10 秒刷新" query={system}>
                {(data) => (
                    <div className="flex flex-col gap-3">
                        <MetricGrid>
                            <Metric label="CPU" value={text(data.cpu_percent) || '—'} unit="%" />
                            <Metric label="内存" value={memoryPercent(data)} unit="%" />
                            <Metric label="磁盘" value={text(data.disk_percent) || '—'} unit="%" />
                        </MetricGrid>
                        <PanelDisclosure label="主机与进程明细">
                            <FieldList data={data} columns={2} />
                        </PanelDisclosure>
                    </div>
                )}
            </PanelQuerySection>
            <PanelSection title="机器人与连接">
                <div className="flex flex-col gap-3">
                    {bots.data?.kind === 'ok' ? (
                        <PanelTable
                            rows={records(bots.data.data.items).map((item) => ({
                                ...item,
                                nickname: item.nickname || item.name,
                            }))}
                            empty="还没有协议连接，连接建立后会显示在这里"
                            columns={[
                                { key: 'user_id', label: 'QQ', mono: true },
                                { key: 'nickname', label: '昵称' },
                                { key: 'platform', label: '平台' },
                                { key: 'status', label: '状态' },
                                { key: 'latency_ms', label: '延迟 ms', align: 'right' },
                            ]}
                        />
                    ) : (
                        <PanelSectionState
                            state={panelQueryState(bots)}
                            onRetry={() => void bots.refetch()}
                        />
                    )}
                    <PanelDisclosure label="当前机器人详情">
                        {bot.data?.kind === 'ok' ? (
                            <FieldList data={bot.data.data} columns={2} />
                        ) : (
                            <PanelSectionState
                                state={panelQueryState(bot)}
                                onRetry={() => void bot.refetch()}
                            />
                        )}
                    </PanelDisclosure>
                </div>
            </PanelSection>
            <PanelQuerySection title="服务" query={services}>
                {(data) => (
                    <PanelTable
                        rows={records(data.items)}
                        empty="面板还没有报告服务状态"
                        columns={[
                            { key: 'name', label: '服务' },
                            { key: 'description', label: '说明' },
                            {
                                key: 'available',
                                label: '状态',
                                render: (row) => (
                                    <Badge
                                        tone={
                                            row.available === true
                                                ? 'success'
                                                : row.available === false
                                                  ? 'warning'
                                                  : 'neutral'
                                        }
                                    >
                                        {row.available === true
                                            ? '可用'
                                            : row.available === false
                                              ? '不可用'
                                              : '未知'}
                                    </Badge>
                                ),
                            },
                        ]}
                    />
                )}
            </PanelQuerySection>
            <PanelQuerySection
                title="后台任务"
                meta="每 10 秒刷新"
                query={tasks}
                actions={
                    <Button size="sm" variant="ghost" onClick={() => onGoTab('scheduled')}>
                        管理定时任务
                    </Button>
                }
            >
                {(data) => (
                    <div className="flex flex-col gap-3">
                        <PanelTable
                            rows={[
                                ...records(data.background).map((item) => ({
                                    ...item,
                                    source: '后台',
                                })),
                                ...records(data.scheduled).map((item) => ({
                                    ...item,
                                    name: item.name ?? item.title,
                                    status: item.status ?? item.state,
                                    source: '定时',
                                })),
                            ]}
                            empty="当前没有后台任务，可前往定时任务添加"
                            columns={[
                                { key: 'name', label: '任务' },
                                { key: 'task_id', label: 'ID', mono: true },
                                { key: 'source', label: '来源' },
                                { key: 'status', label: '状态' },
                                { key: 'pipeline_key', label: '对话流', mono: true },
                                { key: 'next_run', label: '下次执行' },
                            ]}
                        />
                        {text(data.error) && (
                            <InlineNote tone="warning">{text(data.error)}</InlineNote>
                        )}
                    </div>
                )}
            </PanelQuerySection>
        </div>
    );
}
