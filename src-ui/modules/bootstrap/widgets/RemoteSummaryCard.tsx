// 概览「远端主机」卡：台数徽章 + 前几台主机名摘要，点整张卡跳远端页。

import React, { useMemo } from 'react';
import { Server, ChevronRight } from 'lucide-react';
import { Card, Badge } from '../../../shared/ui';
import { MotionIcon } from '../../../shared/ui/motion';
import type { ServerProfile } from '../../../core/ipc/generated/domain/ServerProfile';
import { serverDisplayName } from '../../../core/domain/bootstrap/overviewFormat';
import type { AppRoute } from '../../../shared/components/next/Sidebar';

const REMOTE_NAMES_SHOWN = 3;

export const RemoteSummaryCard: React.FC<{
    servers: ServerProfile[];
    isLoading: boolean;
    onNavigate?: (route: AppRoute) => void;
}> = ({ servers, isLoading, onNavigate }) => {
    const count = servers.length;
    const description = useMemo(() => {
        if (count === 0) return '添加 SSH 档案后，可在组件页向远端一键部署 NapCat。';
        const names = servers.slice(0, REMOTE_NAMES_SHOWN).map(serverDisplayName);
        const rest = count - names.length;
        return rest > 0 ? `${names.join(' · ')} +${rest}` : names.join(' · ');
    }, [servers, count]);

    const countBadge = isLoading ? (
        <span className="inline-flex h-5 min-w-[2.5rem] items-center justify-center rounded-full bg-inset/80 px-2 text-[11px] text-text-tertiary">
            …
        </span>
    ) : count === 0 ? (
        <span className="inline-flex h-5 items-center rounded-full bg-inset/80 px-2 text-[11px] font-medium text-text-tertiary">
            未配置
        </span>
    ) : (
        <Badge tone="info" appearance="soft" className="font-mono text-2xs">
            {count} 台
        </Badge>
    );

    return (
        <Card
            padding="md"
            hover="lift"
            className="cursor-pointer transition-shadow hover:shadow-popover"
            onClick={() => onNavigate?.('remote')}
        >
            <div className="flex items-start gap-3">
                <div className="grid h-9 w-9 shrink-0 place-items-center rounded-md border border-info/20 bg-info/10 text-info">
                    <MotionIcon icon={Server} motion="breathe" playEnter={false} size={18} />
                </div>
                <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-0.5">
                        <div className="flex items-center gap-2">
                            <p className="font-display text-[14.5px] font-semibold text-text">
                                远端主机
                            </p>
                            {countBadge}
                        </div>
                        <span className="text-2xs font-medium text-brand inline-flex items-center gap-0.5">
                            管理
                            <ChevronRight size={12} />
                        </span>
                    </div>
                    <p className="mt-1 truncate text-[12px] leading-snug text-text-tertiary">
                        {description}
                    </p>
                </div>
            </div>
        </Card>
    );
};
