// 人格 / 知识库 / 规则这几个页只有 Dashboard 通了才能读写，这里画不通的原因，并把能做的动作放旁边。

import { KeyRound, Play, Power, Unplug } from 'lucide-react';
import type { ComponentType } from 'react';
import type { LucideProps } from 'lucide-react';
import { Button, Spinner } from '../../../../shared/ui';
import { JumpLink } from './parts';
import type { AstrBotDashboardGate, AstrBotDashboardStatus } from '../../../../core/ipc/types';

type GateLook = {
    icon: ComponentType<LucideProps>;
    title: string;
    box: string;
    well: string;
};

const LOOK: Record<Exclude<AstrBotDashboardGate, 'ok'>, GateLook> = {
    not_running: {
        icon: Power,
        title: '实例没在运行',
        box: 'border-border-subtle bg-inset/60',
        well: 'bg-surface text-text-tertiary',
    },
    auth: {
        icon: KeyRound,
        title: '控制台没登上',
        box: 'border-warning/25 bg-warning-soft/50',
        well: 'bg-warning-soft text-warning',
    },
    unreachable: {
        icon: Unplug,
        title: '连不上控制台',
        box: 'border-danger/25 bg-danger-soft/50',
        well: 'bg-danger-soft text-danger',
    },
};

export const AstrBotRuntimeGate: React.FC<{
    status: AstrBotDashboardStatus | undefined;
    loading?: boolean;
    onGoTab?: (tab: string) => void;
    onStart?: () => void;
    starting?: boolean;
}> = ({ status, loading, onGoTab, onStart, starting }) => {
    if (loading && !status) {
        return (
            <div className="flex items-center gap-2.5 rounded-md border border-border-subtle bg-inset/60 px-3.5 py-3">
                <Spinner size="sm" tone="muted" />
                <p className="text-[13px] text-text-secondary">正在连接 AstrBot 控制台</p>
            </div>
        );
    }
    if (!status || status.gate === 'ok') return null;

    const look = LOOK[status.gate];
    const Icon = look.icon;
    return (
        <div className={`flex items-center gap-3 rounded-md border px-3.5 py-3 ${look.box}`}>
            <span className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-md ${look.well}`}>
                <Icon size={15} />
            </span>
            <div className="min-w-0 flex-1">
                <p className="text-[13px] font-medium text-text">{look.title}</p>
                {status.message && (
                    <p className="mt-0.5 text-xs leading-relaxed text-text-secondary">{status.message}</p>
                )}
            </div>
            {status.gate === 'not_running' && onStart && (
                <Button size="sm" variant="secondary" disabled={starting} onClick={onStart}>
                    {starting ? <Spinner size="xs" /> : <Play size={13} />}
                    启动
                </Button>
            )}
            {status.gate === 'auth' && onGoTab && (
                <JumpLink tab="connections" onGo={onGoTab} className="shrink-0 text-xs">
                    去填 WebUI 密码
                </JumpLink>
            )}
        </div>
    );
};

export function dashboardReady(status: AstrBotDashboardStatus | undefined): boolean {
    return status?.gate === 'ok';
}
