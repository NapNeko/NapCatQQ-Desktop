// 配置页顶栏：返回、头像、实例名与状态徽章、宿主摘要、删除入口。

import { ArrowLeft, Trash2 } from 'lucide-react';
import { Badge, Button } from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { useServerManager } from '../../../../hooks/remote/useServerManager';
import {
    isRuntimeTargetLocal,
    runtimeTargetDisplayLabel,
} from '../../../../core/domain/bot/runtime-target';
import { botProcessBadge } from '../../../../core/domain/bot/bot-status-presentation';
import type { BotActorState } from '../../../../core/ipc/generated/BotActorState';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
import type { DeploymentType } from '../../../../core/ipc/generated/domain/DeploymentType';
import { BotAvatar } from '../../BotAvatar';

interface BotConfigHeaderProps {
    isEditMode: boolean;
    tourDemoMode: boolean;
    botId: string | null;
    qqId?: number;
    botName?: string;
    backendType: BackendType;
    runtimeTarget: string;
    deploymentType?: DeploymentType;
    botState?: BotActorState | null;
    pendingRestart?: boolean;
    onBack: () => void;
    onRequestDelete: () => void;
}

export function BotConfigHeader({
    isEditMode,
    tourDemoMode,
    botId,
    qqId = 0,
    botName = '',
    backendType,
    runtimeTarget,
    deploymentType = 'native',
    botState = null,
    pendingRestart = false,
    onBack,
    onRequestDelete,
}: BotConfigHeaderProps) {
    const { servers } = useServerManager();
    const isSL = backendType === 'snowluma';
    const effectiveQq = botId ?? (qqId > 0 ? String(qqId) : '');
    const trimmedName = botName.trim();

    const title = tourDemoMode
        ? '新建 Bot（演示）'
        : isEditMode
          ? trimmedName || (botId ? `Bot ${botId}` : '编辑 Bot 配置')
          : trimmedName || '新建 Bot';

    const hostLabel = runtimeTargetDisplayLabel(runtimeTarget, servers);
    const isRemote = !isRuntimeTargetLocal(runtimeTarget);
    const deployLabel = isRemote && deploymentType === 'docker' ? 'Docker' : '直接运行';
    const lifecycle = isEditMode && botState ? botProcessBadge(botState) : null;

    const subtitle = tourDemoMode
        ? '已预填演示数据，点保存不会写入配置'
        : effectiveQq
          ? `QQ ${effectiveQq} · ${hostLabel} · ${deployLabel}`
          : '填写账号身份，并至少添加一个协议连接';

    return (
        <header
            className="flex shrink-0 items-center justify-between gap-3 px-2 pb-2 pt-1.5"
            data-tour-id="bot-config-header"
        >
            <div className="flex min-w-0 items-center gap-2.5">
                <Button
                    variant="ghost"
                    size="icon"
                    onClick={onBack}
                    aria-label="返回列表"
                    title="返回列表"
                    disabled={tourDemoMode}
                >
                    <ActionMotionIcon icon={ArrowLeft} size={16} />
                </Button>
                <BotAvatar
                    qqid={effectiveQq}
                    displayName={trimmedName || effectiveQq}
                    flavorTone={isSL ? 'info' : 'brand'}
                    size="sm"
                />
                <div className="flex min-w-0 flex-col gap-0.5">
                    <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                        <h1 className="truncate font-display text-[15px] font-semibold leading-snug text-text">
                            {title}
                        </h1>
                        <Badge
                            tone={isSL ? 'info' : 'brand'}
                            appearance="soft"
                            className="shrink-0"
                        >
                            {isSL ? 'SnowLuma' : 'NapCat'}
                        </Badge>
                        {lifecycle && (
                            <Badge
                                tone={lifecycle.tone}
                                appearance="soft"
                                dot={lifecycle.dot}
                                className="shrink-0"
                            >
                                {lifecycle.label}
                            </Badge>
                        )}
                        {pendingRestart && (
                            <Badge tone="warning" appearance="soft" dot className="shrink-0">
                                待重启
                            </Badge>
                        )}
                    </div>
                    <p className="truncate text-2xs text-text-tertiary">{subtitle}</p>
                </div>
            </div>
            {isEditMode && !tourDemoMode && (
                <Button
                    variant="ghost"
                    size="sm"
                    className="shrink-0 text-danger hover:bg-danger-soft hover:text-danger"
                    onClick={onRequestDelete}
                >
                    <ActionMotionIcon icon={Trash2} size={13} strokeWidth={2.2} />
                    <span>删除实例</span>
                </Button>
            )}
        </header>
    );
}
