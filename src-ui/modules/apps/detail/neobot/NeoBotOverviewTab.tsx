// NeoBot 详情「概览」页：面板首页那几个数（在线 / 版本 / 消息量 / 插件 / 延迟 / 运行时长）。
//
// 数据走 panelCall → /api/overview。没填面板密码或面板没起来时，这里给的是**可操作的提示**
// 而不是「加载失败」——凭据卡片就在本页上方，指过去即可。

import { Button } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { formatUptime } from './neobotPanel';
import { useNeoBotOverview, type PanelState } from './useNeoBotPanel';

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

const Hint: React.FC<{ title: string; children: React.ReactNode }> = ({ title, children }) => (
    <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
        <h3 className="text-sm font-semibold text-text">{title}</h3>
        <p className="mt-1 text-xs leading-relaxed text-text-secondary">{children}</p>
    </section>
);

/** 面板没给可用数据时，按状态给一句能照做的话 */
function blockedHint(state: PanelState): { title: string; body: string } | null {
    switch (state.kind) {
        case 'unsupported':
            return {
                title: '该框架不支持面板转发',
                body: '这个框架没有可由桌面端读取的控制台接口，或用的是另一套对接方式。',
            };
        case 'unauthorized':
            return {
                title: '先填面板密码',
            body: '面板接口需要登录。请在上方「面板凭据」里填入你在 NeoBot 面板登录时用的密码——桌面端只把它存进本机密钥库，不会写回 NeoBot。',
            };
        case 'unreachable':
            return {
                title: '面板打不通',
                body: '实例可能没在运行，或者面板端口与桌面端读到的不一致。先确认实例是运行中，再点刷新。',
            };
        case 'failed':
            return { title: '面板返回了错误', body: state.message || '面板拒绝了这次请求。' };
        case 'malformed':
            return {
                title: '面板的返回不认识',
                body: '面板答了，但结构不是桌面端预期的样子——多半是面板改版了，桌面端这边要跟着更新。',
            };
        default:
            return null;
    }
}

export const NeoBotOverviewTab: React.FC<{ instanceId: string }> = ({ instanceId }) => {
    const query = useNeoBotOverview(instanceId);
    const state = query.data;

    if (query.isLoading) {
        return <p className="text-xs text-text-tertiary">正在读取面板…</p>;
    }
    if (query.isError) {
        return (
            <Hint title="读取失败">
                {query.error.message}
                <Button className="ml-2" size="sm" variant="secondary" onClick={() => void query.refetch()}>
                    重试
                </Button>
            </Hint>
        );
    }
    if (!state) return null;

    const blocked = blockedHint(state);
    if (blocked) {
        return (
            <Hint title={blocked.title}>
                {blocked.body}
                <Button className="ml-2" size="sm" variant="secondary" onClick={() => void query.refetch()}>
                    刷新
                </Button>
            </Hint>
        );
    }
    if (state.kind !== 'ok') return null;
    const o = state.overview;

    return (
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
                            {n.hint && <p className="mt-0.5 text-2xs text-text-tertiary">{n.hint}</p>}
                        </div>
                    ))}
                </section>
            )}
        </div>
    );
};