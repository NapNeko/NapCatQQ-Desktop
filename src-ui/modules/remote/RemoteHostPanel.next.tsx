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

import React, { useRef, useState } from 'react';
import { Server, RefreshCw, Plus, Eye, EyeOff } from 'lucide-react';
import { useGSAP } from '@gsap/react';
import { animateListChildrenEnter } from '../../shared/ui/motion/listEnter';
import { Button, Tooltip, TooltipTrigger, TooltipContent } from '../../shared/ui';
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from '../../shared/ui';
import { ListItem, ActionMotionIcon, RESOURCE_MOTION, refreshMotion } from '../../shared/ui/motion';
import { useMotion } from '../../hooks/preferences/useMotion';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { pushInfoBar } from '../../hooks/ui/globalInfoBarStore';
import { ServerCard } from './ServerCard';
import { AddServerDialog } from './AddServerDialog';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

export const RemoteHostPanelNext: React.FC = () => {
    const {
        servers,
        isLoading,
        refetch,
        addServerAsync,
        isAdding,
        updateServer,
        isUpdating,
        deleteServer,
        testConnection,
        isTesting,
        setupKeyAuth,
        isSettingUpKey,
    } = useServerManager();

    // 表单弹窗：editingProfile=null 走新增，非空走编辑同一弹窗。
    const [formOpen, setFormOpen] = useState(false);
    const [editingProfile, setEditingProfile] = useState<ServerProfile | null>(null);
    const [testingId, setTestingId] = useState<string | null>(null);
    const [revealIp, setRevealIp] = useState(false);

    const openAdd = () => {
        setEditingProfile(null);
        setFormOpen(true);
    };
    const openEdit = (profile: ServerProfile) => {
        setEditingProfile(profile);
        setFormOpen(true);
    };

    const handleTest = (id: string, password?: string) => {
        setTestingId(id);
        testConnection({ id, password });
    };

    // 配置免密：用密码连一次，把公钥写进远端 authorized_keys，档案切到密钥认证。
    const runKeySetup = async (id: string, password: string) => {
        try {
            const updated = await setupKeyAuth({ id, password });
            pushInfoBar({
                tone: 'success',
                title: '免密登录已配置',
                content: `${updated.name || updated.host} 之后用密钥连接，不再需要密码`,
                autoDismissMs: 5000,
            });
        } catch (err) {
            pushInfoBar({
                tone: 'danger',
                title: '配置免密登录失败',
                content: err instanceof Error ? err.message : String(err),
            });
        }
    };

    // 已添加服务器卡片上点"配置免密"时，弹这个小框输入当前密码。
    const [keyAuthTarget, setKeyAuthTarget] = useState<ServerProfile | null>(null);

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
                                <ActionMotionIcon
                                    icon={revealIp ? Eye : EyeOff}
                                    size={14}
                                />
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
                        <ActionMotionIcon
                            icon={RefreshCw}
                            size={14}
                            motion={refreshMotion(isLoading)}
                        />
                        刷新
                    </Button>
                </div>
            </header>

            <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pb-24 pt-1">
                {isLoading && servers.length === 0 ? (
                    <LoadingState />
                ) : servers.length === 0 ? (
                    <EmptyState onCreate={openAdd} />
                ) : (
                    <ServerGrid
                        servers={servers}
                        isTesting={isTesting}
                        testingId={testingId}
                        revealIp={revealIp}
                        handleTest={handleTest}
                        openEdit={openEdit}
                        setKeyAuthTarget={setKeyAuthTarget}
                        deleteServer={deleteServer}
                    />
                )}
            </div>

            <FloatingAddButton onClick={openAdd} />

            <AddServerDialog
                open={formOpen}
                onOpenChange={setFormOpen}
                isSubmitting={editingProfile ? isUpdating : isAdding}
                initialProfile={editingProfile}
                onSubmit={(profile, password, autoKey) => {
                    if (editingProfile) {
                        updateServer({ profile, password });
                        setFormOpen(false);
                        return;
                    }
                    setFormOpen(false);
                    // 新增：先 add 拿到带 id 的档案；勾了自动免密就接着配。
                    void addServerAsync({ profile, password })
                        .then((created) => {
                            if (autoKey && password) {
                                void runKeySetup(created.id, password);
                            }
                        })
                        .catch((err) => {
                            pushInfoBar({
                                tone: 'danger',
                                title: '添加服务器失败',
                                content: err instanceof Error ? err.message : String(err),
                            });
                        });
                }}
            />

            <KeyAuthPasswordDialog
                target={keyAuthTarget}
                isSubmitting={isSettingUpKey}
                onClose={() => setKeyAuthTarget(null)}
                onConfirm={(password) => {
                    const target = keyAuthTarget;
                    setKeyAuthTarget(null);
                    if (target) void runKeySetup(target.id, password);
                }}
            />
        </div>
    );
};

// ─── 子件 ──────────────────────────────────────────────────────────────

/// 服务器卡片网格 + stagger 进出场动画。卡片删除时走 ListItem 的 exit。
function ServerGrid({
    servers,
    isTesting,
    testingId,
    revealIp,
    handleTest,
    openEdit,
    setKeyAuthTarget,
    deleteServer,
}: {
    servers: ServerProfile[];
    isTesting: boolean;
    testingId: string | null;
    revealIp: boolean;
    handleTest: (id: string, pw?: string) => void;
    openEdit: (s: ServerProfile) => void;
    setKeyAuthTarget: (s: ServerProfile | null) => void;
    deleteServer: (id: string) => void;
}) {
    const m = useMotion();
    const containerRef = useRef<HTMLDivElement>(null);

    useGSAP(
        () => {
            const root = containerRef.current;
            if (!root) return;
            animateListChildrenEnter(root, servers.length, m);
        },
        { scope: containerRef, dependencies: [servers.length, m.enabled, m.level] },
    );

    return (
        <div
            ref={containerRef}
            className="grid gap-3"
            style={{
                gridTemplateColumns:
                    'repeat(auto-fill, minmax(min(360px, 100%), 1fr))',
            }}
        >
            {servers.map((server) => (
                <ListItem key={server.id} hoverable>
                    <ServerCard
                        server={server}
                        isTesting={isTesting && testingId === server.id}
                        revealIp={revealIp}
                        onTest={(pw) => handleTest(server.id, pw)}
                        onEdit={() => openEdit(server)}
                        onSetupKey={
                            server.authMethod === 'password'
                                ? () => setKeyAuthTarget(server)
                                : undefined
                        }
                        onDelete={() => deleteServer(server.id)}
                    />
                </ListItem>
            ))}
        </div>
    );
}

function LoadingState() {
    return (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 py-20 text-text-tertiary">
            <ActionMotionIcon icon={RefreshCw} size={20} motion="spin" />
            <p className="text-sm">正在加载服务器档案…</p>
        </div>
    );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
    return (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 rounded-md border border-dashed border-border-subtle bg-elevated/50 py-16 text-center">
            <ActionMotionIcon
                icon={Server}
                size={32}
                strokeWidth={1.6}
                motion={RESOURCE_MOTION}
                className="text-text-tertiary"
            />
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
            <ActionMotionIcon icon={Plus} size={20} strokeWidth={2.4} />
        </button>
    );
}

// 已添加服务器配置免密时弹的密码输入框。只问当前 SSH 密码，确认后把公钥推到
// 远端。target 为 null 时不渲染。
function KeyAuthPasswordDialog({
    target,
    isSubmitting,
    onClose,
    onConfirm,
}: {
    target: ServerProfile | null;
    isSubmitting: boolean;
    onClose: () => void;
    onConfirm: (password: string) => void;
}) {
    const [password, setPassword] = useState('');
    React.useEffect(() => {
        if (target) setPassword('');
    }, [target]);

    if (!target) return null;

    return (
        <Dialog open onOpenChange={(o) => !o && onClose()}>
            <DialogContent className="max-w-sm" dismissOnOutsideClick={false}>
                <DialogHeader>
                    <DialogTitle>配置免密登录</DialogTitle>
                    <DialogDescription>
                        输入 {target.name || target.host} 的当前 SSH 密码。会用它连一次，把本机
                        密钥写进远端，之后免密码连接。
                    </DialogDescription>
                </DialogHeader>
                <form
                    onSubmit={(e) => {
                        e.preventDefault();
                        if (password) onConfirm(password);
                    }}
                >
                    <input
                        type="password"
                        autoFocus
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                        placeholder="SSH 密码"
                        className="h-9 w-full rounded-sm bg-inset px-3 text-sm text-text outline-none transition-colors placeholder:text-text-tertiary focus:ring-1 focus:ring-brand"
                    />
                    <DialogFooter>
                        <Button size="sm" variant="ghost" type="button" onClick={onClose}>
                            取消
                        </Button>
                        <Button
                            size="sm"
                            variant="primary"
                            type="submit"
                            disabled={!password || isSubmitting}
                        >
                            {isSubmitting ? '配置中…' : '配置免密'}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    );
}

export type { ServerProfile };
