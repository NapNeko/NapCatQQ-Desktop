// 单台服务器卡片：品牌 Server 图标 + 紧凑信息区 + 底栏操作。

import React from 'react';
import {
    Globe,
    KeyRound,
    Pencil,
    Server,
    Shield,
    Trash2,
    Wifi,
} from 'lucide-react';
import {
    ActionMotionIcon,
    LIVE_MOTION,
    RESOURCE_MOTION,
    refreshMotion,
} from '../../shared/ui/motion';
import { Tooltip, TooltipTrigger, TooltipContent } from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { BotManageCard } from '../bot/list/next/BotManageCard';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import type { ServerState } from '../../core/ipc/generated/domain/ServerState';
import { serverLifecycleBadge } from './serverCardPresentation';

export { serverCardGridClass } from './serverCardGrid';

interface ServerCardProps {
    server: ServerProfile;
    isTesting: boolean;
    revealIp: boolean;
    onTest: (password?: string) => void;
    onEdit: () => void;
    onSetupKey?: () => void;
    onDelete: () => void;
}

export const ServerCard: React.FC<ServerCardProps> = ({
    server,
    isTesting,
    revealIp,
    onTest,
    onEdit,
    onSetupKey,
    onDelete,
}) => {
    const displayHost = revealIp ? server.host : maskHost(server.host);
    const serverLabel = server.name || server.host;
    const displayName =
        server.name.trim().length > 0
            ? server.name.trim()
            : revealIp
              ? server.host
              : '远端服务器';

    const accent = cardAccent(server.state);
    const sshEndpoint = `${server.username}@${displayHost}:${server.port}`;

    let webuiLine: string | null = null;
    let webuiFull: string | null = null;
    if (server.webuiUrl) {
        try {
            const u = new URL(server.webuiUrl);
            webuiLine = revealIp ? u.host : maskHost(u.host);
            webuiFull = server.webuiUrl;
        } catch {
            webuiLine = revealIp ? server.webuiUrl : 'WebUI';
            webuiFull = server.webuiUrl;
        }
    }

    const authLabel = server.authMethod === 'key' ? 'SSH 密钥' : '密码登录';

    const stop = (fn: () => void) => (e: React.MouseEvent) => {
        e.stopPropagation();
        fn();
    };

    return (
        <BotManageCard
            compact
            status={{
                lifecycle: serverLifecycleBadge(server.state),
                session: null,
                alert: null,
            }}
            accent={accent}
            header={
                <>
                    <div className="grid h-9 w-9 shrink-0 place-items-center rounded-md bg-brand-soft text-brand">
                        <ActionMotionIcon icon={Server} size={18} motion={RESOURCE_MOTION} />
                    </div>
                    <div className="min-w-0 flex-1">
                        <h3
                            className="truncate font-display text-base font-semibold leading-snug text-text"
                            title={serverLabel}
                        >
                            {displayName}
                        </h3>
                        <p className="mt-0.5 flex min-w-0 items-center gap-1.5 text-2xs text-text-tertiary">
                            <span className="font-medium text-info">远端</span>
                            <span aria-hidden className="text-border">
                                ·
                            </span>
                            <span className="truncate font-mono tabular-nums">
                                {server.username}
                            </span>
                        </p>
                    </div>
                </>
            }
            meta={
                isTesting ? (
                    <p className="truncate text-xs text-brand">正在测试连接…</p>
                ) : (
                    <p
                        className="truncate font-mono text-xs text-text-secondary"
                        title={
                            revealIp
                                ? `${server.username}@${server.host}:${server.port}`
                                : undefined
                        }
                    >
                        {sshEndpoint}
                    </p>
                )
            }
            chips={
                <ServerDetailStrip
                    revealIp={revealIp}
                    webuiLine={webuiLine}
                    webuiFull={webuiFull}
                    authLabel={authLabel}
                    credentialSaved={server.rememberCredential}
                />
            }
            footerActions={
                <>
                    <ServerIconButton
                        tooltip="测试连接"
                        onClick={stop(() => onTest())}
                        disabled={isTesting}
                        tone={server.state === 'connected' ? 'success' : 'neutral'}
                    >
                        <ActionMotionIcon
                            icon={Wifi}
                            size={16}
                            strokeWidth={2.2}
                            motion={
                                isTesting
                                    ? refreshMotion(true)
                                    : server.state === 'connected'
                                      ? LIVE_MOTION
                                      : 'none'
                            }
                        />
                    </ServerIconButton>
                    {onSetupKey ? (
                        <ServerIconButton
                            tooltip="配置免密登录"
                            onClick={stop(onSetupKey)}
                        >
                            <ActionMotionIcon icon={KeyRound} size={16} strokeWidth={2} />
                        </ServerIconButton>
                    ) : null}
                    <ServerIconButton tooltip="编辑" onClick={stop(onEdit)}>
                        <ActionMotionIcon icon={Pencil} size={16} strokeWidth={2} />
                    </ServerIconButton>
                    <ServerIconButton
                        tooltip="删除"
                        onClick={stop(onDelete)}
                        tone="danger"
                    >
                        <ActionMotionIcon icon={Trash2} size={16} strokeWidth={2.2} />
                    </ServerIconButton>
                </>
            }
        />
    );
};

function ServerDetailStrip({
    revealIp,
    webuiLine,
    webuiFull,
    authLabel,
    credentialSaved,
}: {
    revealIp: boolean;
    webuiLine: string | null;
    webuiFull: string | null;
    authLabel: string;
    credentialSaved: boolean;
}) {
    return (
        <span className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-0.5 text-2xs text-text-secondary">
            <span className="inline-flex items-center gap-1">
                <KeyRound size={11} strokeWidth={2.4} className="text-text-tertiary" />
                {authLabel}
            </span>
            {credentialSaved ? (
                <Tooltip>
                    <TooltipTrigger asChild>
                        <span className="inline-flex cursor-default items-center gap-1 text-success">
                            <Shield size={11} strokeWidth={2.4} />
                            凭据已保存
                        </span>
                    </TooltipTrigger>
                    <TooltipContent>登录凭据已安全存储在本机</TooltipContent>
                </Tooltip>
            ) : null}
            {webuiLine ? (
                <Tooltip>
                    <TooltipTrigger asChild>
                        <span className="inline-flex min-w-0 max-w-full cursor-default items-center gap-1">
                            <Globe size={11} strokeWidth={2.4} className="shrink-0 text-text-tertiary" />
                            <span className="truncate font-mono">{webuiLine}</span>
                        </span>
                    </TooltipTrigger>
                    <TooltipContent>
                        {revealIp && webuiFull ? webuiFull : '显示 IP 后可查看完整 WebUI 地址'}
                    </TooltipContent>
                </Tooltip>
            ) : null}
        </span>
    );
}

function ServerIconButton({
    tooltip,
    onClick,
    disabled,
    tone = 'neutral',
    children,
}: {
    tooltip: string;
    onClick: (e: React.MouseEvent) => void;
    disabled?: boolean;
    tone?: 'neutral' | 'success' | 'danger';
    children: React.ReactNode;
}) {
    return (
        <Tooltip>
            <TooltipTrigger asChild>
                <button
                    type="button"
                    onClick={onClick}
                    disabled={disabled}
                    className={cn(
                        'inline-flex h-9 w-9 items-center justify-center rounded-sm transition-colors duration-100',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                        'disabled:cursor-not-allowed disabled:opacity-40',
                        tone === 'neutral' &&
                            'text-text-secondary hover:bg-inset hover:text-text',
                        tone === 'success' && 'text-success hover:bg-success-soft',
                        tone === 'danger' && 'text-danger hover:bg-danger-soft',
                    )}
                >
                    {children}
                </button>
            </TooltipTrigger>
            <TooltipContent>{tooltip}</TooltipContent>
        </Tooltip>
    );
}

function cardAccent(state: ServerState): 'brand' | 'danger' | 'none' {
    if (state === 'failed') return 'danger';
    if (state === 'connected') return 'brand';
    return 'none';
}

function maskHost(host: string): string {
    if (host.length < 4) return '····';
    return `${host[0]}${'·'.repeat(Math.min(host.length - 2, 12))}${host[host.length - 1]}`;
}