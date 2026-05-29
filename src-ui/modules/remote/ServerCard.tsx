// 单台服务器卡片。承载 ServerProfile 的连接信息 + 测试 / 删除操作。
//
// 视觉对齐 ComponentCard：Card padding="md" + header 行 + 副信息行 + 操作区。
// 状态徽章用 Badge tone 映射：connected=success / failed=danger /
// connecting=warning / disconnected=neutral。
//
// 操作按钮：测试连接（始终可用） / 删除（hover 出红）。编辑入口暂留给 v2。

import React from 'react';
import { Server, Wifi, Trash2, Loader2 } from 'lucide-react';
import { Card, Badge, Button, Tooltip, TooltipTrigger, TooltipContent } from '../../shared/ui';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import type { ServerState } from '../../core/ipc/generated/domain/ServerState';

interface ServerCardProps {
    server: ServerProfile;
    isTesting: boolean;
    onTest: (password?: string) => void;
    onDelete: () => void;
}

export const ServerCard: React.FC<ServerCardProps> = ({
    server,
    isTesting,
    onTest,
    onDelete,
}) => {
    const stateMeta = stateBadge(server.state);

    return (
        <Card padding="md" className="flex flex-col gap-3">
            <header className="flex items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                        <Server size={16} className="shrink-0 text-text-tertiary" />
                        <h3 className="truncate font-display text-base font-semibold text-text">
                            {server.name || server.host}
                        </h3>
                        <Badge tone={stateMeta.tone} appearance="soft" dot={server.state === 'connected'}>
                            {stateMeta.label}
                        </Badge>
                    </div>
                    <p className="mt-1 truncate font-mono text-[12.5px] text-text-tertiary tabular-nums">
                        {server.username}@{server.host}:{server.port}
                    </p>
                </div>
            </header>

            <div className="flex flex-wrap items-center gap-2 text-2xs text-text-tertiary">
                <span className="rounded-pill bg-inset px-2 py-0.5">
                    {server.authMethod === 'key' ? '密钥认证' : '密码认证'}
                </span>
                {server.rememberCredential && (
                    <span className="rounded-pill bg-inset px-2 py-0.5">已保存凭据</span>
                )}
                {server.webuiUrl && (
                    <span className="rounded-pill bg-inset px-2 py-0.5">
                        WebUI {new URL(server.webuiUrl).host}
                    </span>
                )}
            </div>

            <div className="flex items-center justify-end gap-1">
                <Tooltip>
                    <TooltipTrigger asChild>
                        <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => onTest()}
                            disabled={isTesting}
                        >
                            {isTesting ? (
                                <Loader2 size={14} className="animate-spin" />
                            ) : (
                                <Wifi size={14} />
                            )}
                            测试连接
                        </Button>
                    </TooltipTrigger>
                    <TooltipContent>
                        建立 SSH 连接并探测远端 OS
                    </TooltipContent>
                </Tooltip>
                <Tooltip>
                    <TooltipTrigger asChild>
                        <button
                            type="button"
                            onClick={onDelete}
                            className="inline-flex h-8 w-8 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-danger-soft hover:text-danger focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-danger"
                            aria-label="删除服务器"
                        >
                            <Trash2 size={14} />
                        </button>
                    </TooltipTrigger>
                    <TooltipContent>
                        删除档案（同时清除已保存的凭据）
                    </TooltipContent>
                </Tooltip>
            </div>
        </Card>
    );
};

function stateBadge(state: ServerState): {
    tone: 'success' | 'danger' | 'warning' | 'neutral';
    label: string;
} {
    switch (state) {
        case 'connected':
            return { tone: 'success', label: '已连接' };
        case 'connecting':
            return { tone: 'warning', label: '连接中' };
        case 'failed':
            return { tone: 'danger', label: '失败' };
        case 'disconnected':
        default:
            return { tone: 'neutral', label: '未连接' };
    }
}
