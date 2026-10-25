import { Badge, Button } from '../../../../shared/ui';
import { formatUptime } from '../../../../core/domain/apps/neobotPanel';
import { PanelStateView } from './PanelStateView';
import { useNeoBotOverview } from '../../../../hooks/apps/useNeoBotPanel';
import { FieldList, InlineNote, Metric, MetricGrid, PanelSection } from './workspaceParts';
import { PanelDisclosure } from './PanelDisclosure';

export const NeoBotOverviewTab: React.FC<{
    instanceId: string;
    onGoTab: (tab: string) => void;
}> = ({ instanceId, onGoTab }) => {
    const query = useNeoBotOverview(instanceId);

    return (
        <PanelStateView
            state={query.data}
            isError={query.isError}
            errorMessage={query.error?.message}
            onRetry={() => void query.refetch()}
            onGoTab={onGoTab}
        >
            {(o) => (
                <div className="flex flex-col gap-4">
                    <PanelSection
                        title={
                            <span className="flex flex-wrap items-center gap-2">
                                <span>{o.botNickname || '未登录'}</span>
                                <Badge
                                    tone={o.standby ? 'warning' : o.online ? 'success' : 'neutral'}
                                >
                                    {o.standby ? '待机中' : o.online ? '在线' : '离线'}
                                </Badge>
                            </span>
                        }
                        meta={
                            <span className="font-mono-strict">{o.botUserId || '尚未连接 QQ'}</span>
                        }
                        actions={
                            <Button
                                size="sm"
                                variant="ghost"
                                disabled={query.isFetching}
                                onClick={() => void query.refetch()}
                            >
                                {query.isFetching ? '刷新中' : '刷新'}
                            </Button>
                        }
                    >
                        <MetricGrid>
                            <Metric
                                label="今日消息"
                                value={o.todayMessages.toLocaleString()}
                                tone="brand"
                            />
                            <Metric label="累计消息" value={o.totalMessages.toLocaleString()} />
                            <Metric label="运行时长" value={formatUptime(o.uptimeSeconds)} />
                            <Metric
                                label="插件"
                                value={o.pluginsLoaded + '/' + o.pluginsTotal}
                                tone={o.pluginsError > 0 ? 'warn' : 'default'}
                            />
                            <Metric
                                label="延迟"
                                value={o.latencyMs === null ? '—' : o.latencyMs + ' ms'}
                            />
                            <Metric
                                label="插件异常"
                                value={o.pluginsError}
                                tone={o.pluginsError > 0 ? 'warn' : 'muted'}
                            />
                        </MetricGrid>
                    </PanelSection>

                    {o.notices.length > 0 && (
                        <section className="flex flex-col gap-1.5">
                            {o.notices.map((n, i) => (
                                <InlineNote
                                    key={n.text + i}
                                    tone={
                                        n.level === 'error'
                                            ? 'danger'
                                            : n.level === 'warning'
                                              ? 'warning'
                                              : 'info'
                                    }
                                    action={
                                        <Button
                                            size="sm"
                                            variant="ghost"
                                            onClick={() =>
                                                onGoTab(
                                                    /插件/.test(n.text)
                                                        ? 'plugins'
                                                        : /模型|供应商/.test(n.text)
                                                          ? 'models'
                                                          : 'config',
                                                )
                                            }
                                        >
                                            查看
                                        </Button>
                                    }
                                >
                                    <p>{n.text}</p>
                                    {n.hint && (
                                        <p className="mt-0.5 text-2xs text-text-tertiary">
                                            {n.hint}
                                        </p>
                                    )}
                                </InlineNote>
                            ))}
                        </section>
                    )}
                    <PanelDisclosure label="运行环境">
                        <FieldList
                            data={{
                                name: o.appName || 'NeoBot',
                                version: o.appVersion,
                                python_version: o.pythonVersion,
                                hostname: o.hostname,
                            }}
                            labels={{
                                name: '应用',
                                version: '版本',
                                python_version: 'Python',
                                hostname: '主机',
                            }}
                            columns={2}
                        />
                    </PanelDisclosure>
                </div>
            )}
        </PanelStateView>
    );
};
