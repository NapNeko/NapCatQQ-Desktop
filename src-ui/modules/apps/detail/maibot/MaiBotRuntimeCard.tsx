// 概览里「在跑」时的一张卡：版本、跑了多久、近 24 小时用量，外加试聊和重启。
// 状态走麦麦自己的 WebUI；它还没起来时说明一句，不当成出错。

import { MessageCircle, RotateCw } from 'lucide-react';
import { Button, Card, Spinner } from '../../../../shared/ui';
import type { AppInstance } from '../../../../core/ipc/types';
import {
    useMaiBotRestart,
    useMaiBotStats,
    useMaiBotStatus,
} from '../../../../hooks/apps/useMaiBotRuntime';

export function formatUptime(secs: number): string {
    const m = Math.floor(secs / 60);
    if (m < 1) return '不到 1 分钟';
    if (m < 60) return `${m} 分钟`;
    const h = Math.floor(m / 60);
    if (h < 24) return m % 60 ? `${h} 小时 ${m % 60} 分` : `${h} 小时`;
    const d = Math.floor(h / 24);
    return h % 24 ? `${d} 天 ${h % 24} 小时` : `${d} 天`;
}

export function formatTokens(n: number): string {
    if (n < 10_000) return `${n}`;
    const w = n / 10_000;
    return `${w >= 100 ? Math.round(w) : w.toFixed(1)} 万`;
}

const Stat: React.FC<{ label: string; value: string }> = ({ label, value }) => (
    <div className="flex min-w-0 flex-col gap-0.5">
        <span className="font-display text-[17px] font-semibold tabular-nums leading-tight text-text">
            {value}
        </span>
        <span className="text-2xs text-text-tertiary">{label}</span>
    </div>
);

export const MaiBotRuntimeCard: React.FC<{
    instance: AppInstance;
    onOpenChat: () => void;
}> = ({ instance, onOpenChat }) => {
    const running = instance.state === 'running';
    const status = useMaiBotStatus(instance.id, running);
    const ok = status.data?.gate === 'ok';
    const stats = useMaiBotStats(instance.id, 24, ok);
    const restart = useMaiBotRestart(instance.id, instance.display_name);
    if (!running) return null;

    const s = status.data;
    const head = ok
        ? [
              s?.version && `v${s.version}`,
              s?.uptime_secs !== undefined && `已运行 ${formatUptime(s.uptime_secs)}`,
          ]
              .filter(Boolean)
              .join(' · ')
        : null;

    return (
        <Card padding="none" className="flex flex-col gap-4 px-5 py-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex min-w-0 items-center gap-2 text-[13px]">
                    {ok ? (
                        <span className="text-text-secondary">{head || '运行中'}</span>
                    ) : (
                        <>
                            {s?.gate === 'unreachable' && <Spinner size="sm" />}
                            <span className="text-text-secondary">
                                {s?.message || '正在连麦麦的 WebUI…'}
                            </span>
                        </>
                    )}
                </div>
                <div className="flex shrink-0 items-center gap-2">
                    <Button size="sm" variant="secondary" disabled={!ok} onClick={onOpenChat}>
                        <MessageCircle size={13} />
                        试聊
                    </Button>
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={!ok || restart.isPending}
                        onClick={() => restart.mutate()}
                    >
                        {restart.isPending ? <Spinner size="sm" /> : <RotateCw size={13} />}
                        重启
                    </Button>
                </div>
            </div>
            {ok && stats.data && (
                <div className="grid grid-cols-2 gap-x-6 gap-y-3 sm:grid-cols-5">
                    <Stat label="24 小时收到消息" value={`${stats.data.total_messages}`} />
                    <Stat label="回复" value={`${stats.data.total_replies}`} />
                    <Stat label="调用模型" value={`${stats.data.total_requests} 次`} />
                    <Stat label="用掉 token" value={formatTokens(stats.data.total_tokens)} />
                    <Stat
                        label="花费（按模型页的价格）"
                        value={
                            stats.data.total_cost > 0 ? `¥${stats.data.total_cost.toFixed(2)}` : '—'
                        }
                    />
                </div>
            )}
        </Card>
    );
};
