// 组件大卡：一个组件 + N 台主机的状态行 + release notes 摘要。
//
// 数据来自 props（domain ComponentRow），动作上抛 onRowAction 由父级编排。

import React from 'react';
import { ExternalLink } from 'lucide-react';
import { Card } from '../../shared/ui';
import {
    HostStatusRowView,
    type RowAction,
    rowActionToStepKind,
} from './HostStatusRow';
import type { ComponentRow } from '../../core/domain/components/types';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { StepKind } from '../../core/ipc/types';

interface ComponentCardProps {
    data: ComponentRow;
    /** 远端最新版本（来自 useReleases），用于派生 "有更新"。 */
    latestRemoteVersion: string | null;
    /** 给定 (component, host) 当前活跃 task 的进度。 */
    getProgress: (
        hostId: string,
    ) => { taskId: string; progress: ActionProgressView } | null;
    /** 行操作上抛：(host_id, kind | cancel) */
    onAction: (
        hostId: string,
        action: { stepKind: StepKind } | { cancelTaskId: string },
    ) => void;
    onRetryDetect: (hostId: string) => void;
}

export const ComponentCard: React.FC<ComponentCardProps> = ({
    data,
    latestRemoteVersion,
    getProgress,
    onAction,
    onRetryDetect,
}) => {
    const handleRowAction = (hostId: string, action: RowAction) => {
        if (action.kind === 'retry_detect') {
            onRetryDetect(hostId);
            return;
        }
        if (action.kind === 'cancel') {
            onAction(hostId, { cancelTaskId: action.taskId });
            return;
        }
        const stepKind = rowActionToStepKind(action);
        if (stepKind) onAction(hostId, { stepKind });
    };

    return (
        <Card padding="md" className="flex flex-col gap-3">
            <header className="flex items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                        <h3 className="truncate font-display text-base font-semibold text-text">
                            {data.info.display_name}
                        </h3>
                        {latestRemoteVersion && (
                            <span className="shrink-0 rounded-pill bg-info-soft px-2 py-0.5 font-mono text-[10.5px] font-medium text-info tabular-nums">
                                latest {latestRemoteVersion}
                            </span>
                        )}
                    </div>
                    <p className="mt-1 line-clamp-2 text-[12.5px] leading-relaxed text-text-tertiary">
                        {data.info.description}
                    </p>
                </div>
                {data.info.repo_url && (
                    <a
                        href={data.info.repo_url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="inline-flex shrink-0 items-center gap-1 rounded-sm px-2 py-1 text-[11.5px] text-text-secondary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand"
                    >
                        仓库
                        <ExternalLink size={11} strokeWidth={2} />
                    </a>
                )}
            </header>

            <div className="flex flex-col gap-1.5">
                {data.rows.map((row) => (
                    <HostStatusRowView
                        key={row.host.host_id}
                        row={row}
                        latestRemoteVersion={latestRemoteVersion}
                        activeProgress={getProgress(row.host.host_id)}
                        onAction={(action) => handleRowAction(row.host.host_id, action)}
                    />
                ))}
            </div>
        </Card>
    );
};

export default ComponentCard;
