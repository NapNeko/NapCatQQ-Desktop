// NeoBot 详情「概览」页：面板首页那几个数（在线 / 版本 / 消息量 / 插件 / 延迟 / 运行时长）。
//
// 四态（没填密码、面板没起来…）由 PanelStateView 统一处理，这里只管成功时的渲染。

import { Button } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { formatUptime } from './neobotPanel';
import { PanelStateView } from './PanelStateView';
import { useNeoBotOverview } from '../../../../hooks/apps/useNeoBotPanel';

const Stat: React.FC<{ label: string; value: string; tone?: 'default' | 'warn' }> = ({
    label,
    value,
    tone = 'default',
}) => (
    <div className="rounded-sm border border-border-subtle bg-inset/40 px-3 py-2">
        <p className="text-2xs text-text-tertiary">{label}</p>
        <p
            className={cn(
                'mt-0.5 truncate text-sm font-medium',
                tone === 'warn' ? 'text-warning' : 'text-text',
            )}
            title={value}
        >
            {value}
        </p>
    </div>
);

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
                <div className="flex flex-col gap-3">
                    <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
                        <div className="flex items-center justify-between gap-2">
                            <div className="flex min-w-0 items-center gap-2">
                                <span
                                    className={cn(
                                        'h-2 w-2 shrink-0 rounded-full',
                                        o.online ? 'bg-brand' : 'bg-text-tertiary',
                                    )}
                                    aria-hidden
                                />
                                <h3 className="truncate text-sm font-semibold text-text">
                                    {o.botNickname || '未登录'}
                                    {o.botUserId ? ' · ' + o.botUserId : ''}
                                </h3>
                                {o.standby && <span className="text-2xs text-warning">待机中</span>}
                            </div>
                            <Button size="sm" variant="ghost" onClick={() => void query.refetch()}>
                                刷新
                            </Button>
                        </div>
                        <p className="mt-1 text-2xs text-text-tertiary">
                            {o.appName || 'NeoBot'} {o.appVersion ? 'v' + o.appVersion : ''}
                            {o.pythonVersion ? ' · Python ' + o.pythonVersion : ''}
                            {o.hostname ? ' · ' + o.hostname : ''}
                        </p>
                    </section>

                    <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                        <Stat label="今日消息" value={String(o.todayMessages)} />
                        <Stat label="累计消息" value={String(o.totalMessages)} />
                        <Stat label="运行时长" value={formatUptime(o.uptimeSeconds)} />
                        <Stat
                            label="插件"
                            value={o.pluginsLoaded + '/' + o.pluginsTotal}
                            tone={o.pluginsError > 0 ? 'warn' : 'default'}
                        />
                        <Stat
                            label="延迟"
                            value={o.latencyMs === null ? '—' : o.latencyMs + ' ms'}
                        />
                        <Stat label="连接状态" value={o.online ? '在线' : '离线'} />
                    </div>

                    {o.notices.length > 0 && (
                        <section className="flex flex-col gap-1.5">
                            {o.notices.map((n, i) => (
                                <div
                                    key={n.text + i}
                                    className="rounded-sm border border-border-subtle bg-inset/40 px-3 py-2"
                                >
                                    <p className="text-xs text-warning">{n.text}</p>
                                    {n.hint && (
                                        <p className="mt-0.5 text-2xs text-text-tertiary">
                                            {n.hint}
                                        </p>
                                    )}
                                </div>
                            ))}
                        </section>
                    )}
                </div>
            )}
        </PanelStateView>
    );
};
