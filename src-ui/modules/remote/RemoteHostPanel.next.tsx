// 远端主机管理页（新树）。
//
// 信息架构：服务器档案为单位，每张卡承载一个 ServerProfile 的连接信息和操作。
// 远端页只管"档案"，不做组件部署——部署走组件页（host_id="remote:<id>"）。
//
// 视觉语言对齐 ComponentsPage / BotListPage：
//   - 头部：2xs uppercase 小标题 + display 大标题 + 副描述
//   - design token：text-text / text-text-secondary / text-text-tertiary / bg-elevated / bg-inset
//   - Section 模式（暂无分组，单 section）+ auto-fill 网格
//   - EmptyState 虚线边框卡 + Bot icon
//   - 添加按钮走右下角悬浮（同 Bot 列表 FloatingActions），添加表单走 Radix Dialog
//
// 严守 frontend-layering：仅 import hooks / shared/ui / 自身组件。

import React, { useState } from 'react';
import { Server, RefreshCw, Plus, Eye, EyeOff } from 'lucide-react';
import { Button, Tooltip, TooltipTrigger, TooltipContent } from '../../shared/ui';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { ServerCard } from './ServerCard';
import { AddServerDialog } from './AddServerDialog';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

export const RemoteHostPanelNext: React.FC = () => {
    const {
        servers,
        isLoading,
        refetch,
        addServer,
        isAdding,
        deleteServer,
        testConnection,
        isTesting,
    } = useServerManager();

    const [addOpen, setAddOpen] = useState(false);
    const [testingId, setTestingId] = useState<string | null>(null);
    const [revealIp, setRevealIp] = useState(false);

    const handleTest = (id: string, password?: string) => {
        setTestingId(id);
        testConnection({ id, password });
    };

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">
                        servers
                    </p>
                    <h1 className="font-display text-xl font-semibold text-text">
                        远端主机
                    </h1>
                    <p className="mt-1 text-sm text-text-secondary">
                        管理 SSH 服务器档案。在组件页选择主机后可在远端部署 NapCat 运行时。
                    </p>
                </div>
                <div className="flex items-center gap-1.5">
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <button
                                type="button"
                                onClick={() => setRevealIp((v) => !v)}
                                className="inline-flex h-8 w-8 items-center justify-center rounded-sm text-text-secondary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                aria-label={revealIp ? '隐藏 IP 信息' : '显示 IP 信息'}
                            >
                                {revealIp ? <Eye size={14} /> : <EyeOff size={14} />}
                            </button>
                        </TooltipTrigger>
                        <TooltipContent>
                            {revealIp ? '隐藏 IP 信息' : '显示 IP 信息'}
                        </TooltipContent>
                    </Tooltip>
                    <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => refetch()}
                        disabled={isLoading}
                    >
                        <RefreshCw size={14} className={isLoading ? 'animate-spin' : undefined} />
                        刷新
                    </Button>
                </div>
            </header>

            <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pb-24 pt-1">
                {isLoading && servers.length === 0 ? (
                    <LoadingState />
                ) : servers.length === 0 ? (
                    <EmptyState onCreate={() => setAddOpen(true)} />
                ) : (
                    <div
                        className="grid gap-3"
                        style={{
                            gridTemplateColumns:
                                'repeat(auto-fill, minmax(min(360px, 100%), 1fr))',
                        }}
                    >
                        {servers.map((server) => (
                            <ServerCard
                                key={server.id}
                                server={server}
                                isTesting={isTesting && testingId === server.id}
                                revealIp={revealIp}
                                onTest={(pw) => handleTest(server.id, pw)}
                                onDelete={() => deleteServer(server.id)}
                            />
                        ))}
                    </div>
                )}
            </div>

            <FloatingAddButton onClick={() => setAddOpen(true)} />

            <AddServerDialog
                open={addOpen}
                onOpenChange={setAddOpen}
                isAdding={isAdding}
                onSubmit={(profile, password) => {
                    addServer({ profile, password });
                    setAddOpen(false);
                }}
            />
        </div>
    );
};

// ─── 子件 ──────────────────────────────────────────────────────────────

function LoadingState() {
    return (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 py-20 text-text-tertiary">
            <RefreshCw size={20} className="animate-spin" />
            <p className="text-sm">正在加载服务器档案…</p>
        </div>
    );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
    return (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 rounded-md border border-dashed border-border-subtle bg-elevated/50 py-16 text-center">
            <Server size={32} strokeWidth={1.6} className="text-text-tertiary" />
            <div>
                <p className="font-display text-md font-semibold text-text">
                    还没有远端服务器
                </p>
                <p className="mt-1 text-xs text-text-secondary">
                    添加一台 SSH 服务器后，就能在组件页把 NapCat 部署到远端。
                </p>
            </div>
            <Button size="sm" variant="primary" onClick={onCreate}>
                添加第一台服务器
            </Button>
        </div>
    );
}

function FloatingAddButton({ onClick }: { onClick: () => void }) {
    return (
        <button
            type="button"
            onClick={onClick}
            className="pointer-events-auto fixed bottom-8 right-8 z-30 inline-flex h-11 w-11 items-center justify-center rounded-full bg-brand text-white shadow-popover transition-all duration-150 hover:scale-105 hover:bg-brand-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
            aria-label="添加服务器"
        >
            <Plus size={20} strokeWidth={2.4} />
        </button>
    );
}

export type { ServerProfile };
