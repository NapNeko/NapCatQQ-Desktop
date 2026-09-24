// 详情页头部：名字 + 运行状态 + 身份行（框架 · 主机端口 · 对接），右侧生命周期按钮和更多菜单。

import {
    ArrowLeft,
    Download,
    ExternalLink,
    Link2,
    MessageSquare,
    MoreHorizontal,
    Play,
    RefreshCw,
    Square,
    Trash2,
    Unlink,
} from 'lucide-react';
import { Badge, Button, Popover, PopoverClose, PopoverContent, PopoverTrigger, Spinner } from '../../../shared/ui';
import { ActionMotionIcon, EMPHASIS_MOTION } from '../../../shared/ui/motion';
import { cn } from '../../../shared/utils/cn';
import type { AppInstance, AppInstanceState } from '../../../core/ipc/types';

const STATE_LOOK: Record<AppInstanceState, { label: string; tone: 'success' | 'warning' | 'neutral' }> = {
    running: { label: '运行中', tone: 'success' },
    installing: { label: '安装中', tone: 'warning' },
    installed: { label: '未启动', tone: 'neutral' },
    stopped: { label: '已停止', tone: 'neutral' },
    not_installed: { label: '未安装', tone: 'neutral' },
};

export const DetailHeader: React.FC<{
    instance: AppInstance;
    frameworkName?: string;
    hostLabel: string;
    installed: boolean;
    busy: boolean;
    canWebUi: boolean;
    onBack: () => void;
    onInstall: () => void;
    onStart: () => void;
    onStop: () => void;
    /** 只有 AstrBot 运行时给；它自带网页对话 */
    onTryChat?: () => void;
    onLink: () => void;
    onUnlink: () => void;
    onWebUi: () => void;
    onRefresh: () => void;
    onDelete: () => void;
}> = ({
    instance,
    frameworkName,
    hostLabel,
    installed,
    busy,
    canWebUi,
    onBack,
    onInstall,
    onStart,
    onStop,
    onTryChat,
    onLink,
    onUnlink,
    onWebUi,
    onRefresh,
    onDelete,
}) => {
    const running = instance.state === 'running';
    const look = STATE_LOOK[instance.state];
    // 默认名就是「AstrBot · 本机」这种，身份行再写一遍框架名是重复
    const showFramework = !!frameworkName && !instance.display_name.includes(frameworkName);
    const identity = [
        showFramework ? frameworkName : null,
        `${hostLabel} :${instance.port}`,
        instance.link ? `已对接 ${instance.link.bot_id}` : null,
    ]
        .filter(Boolean)
        .join(' · ');

    return (
        <header className="flex items-start justify-between gap-3 border-b border-border-subtle py-3">
            <div className="flex min-w-0 items-start gap-3">
                <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回列表">
                    <ActionMotionIcon icon={ArrowLeft} size={16} />
                </Button>
                <div className="flex min-w-0 flex-col gap-0.5">
                    <div className="flex min-w-0 items-center gap-2">
                        <h1 className="truncate font-display text-md font-semibold text-text">{instance.display_name}</h1>
                        <Badge tone={look.tone} appearance="soft" className="shrink-0">
                            {look.label}
                        </Badge>
                    </div>
                    <p className="truncate text-xs text-text-tertiary" title={instance.install_dir}>
                        {identity}
                    </p>
                </div>
            </div>

            <div className="flex shrink-0 items-center gap-1.5">
                {busy && <Spinner size="sm" />}
                {!installed && (
                    <Button size="sm" variant="primary" disabled={busy} onClick={onInstall}>
                        <ActionMotionIcon icon={Download} size={13} motion={EMPHASIS_MOTION} />
                        安装
                    </Button>
                )}
                {onTryChat && (
                    <Button size="sm" variant="ghost" disabled={busy} onClick={onTryChat}>
                        <ActionMotionIcon icon={MessageSquare} size={13} />
                        试聊
                    </Button>
                )}
                {installed && !running && (
                    <Button size="sm" variant="secondary" disabled={busy} onClick={onStart}>
                        <ActionMotionIcon icon={Play} size={13} motion={EMPHASIS_MOTION} />
                        启动
                    </Button>
                )}
                {running && (
                    <Button size="sm" variant="secondary" disabled={busy} onClick={onStop}>
                        <ActionMotionIcon icon={Square} size={13} />
                        停止
                    </Button>
                )}
                <Popover>
                    <PopoverTrigger asChild>
                        <Button variant="ghost" size="icon" className="h-8 w-8" disabled={busy} aria-label="更多">
                            <ActionMotionIcon icon={MoreHorizontal} size={16} />
                        </Button>
                    </PopoverTrigger>
                    <PopoverContent align="end" sideOffset={6} className="w-44 p-1">
                        {installed && <MoreItem icon={Link2} label={instance.link ? '改绑' : '对接'} onClick={onLink} />}
                        {instance.link && <MoreItem icon={Unlink} label="解除对接" onClick={onUnlink} />}
                        {canWebUi && <MoreItem icon={ExternalLink} label="打开 WebUI" onClick={onWebUi} />}
                        <MoreItem icon={RefreshCw} label="重新探测" onClick={onRefresh} />
                        <div className="my-1 h-px bg-border-subtle" />
                        <MoreItem
                            icon={Trash2}
                            label={instance.origin === 'imported' ? '释放接管' : '删除实例'}
                            tone="danger"
                            onClick={onDelete}
                        />
                    </PopoverContent>
                </Popover>
            </div>
        </header>
    );
};

const MoreItem: React.FC<{
    icon: typeof Link2;
    label: string;
    tone?: 'neutral' | 'danger';
    onClick: () => void;
}> = ({ icon: Icon, label, tone = 'neutral', onClick }) => (
    <PopoverClose asChild>
        <button
            type="button"
            className={cn(
                'flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[13px]',
                tone === 'danger' ? 'text-danger hover:bg-danger-soft' : 'text-text hover:bg-inset',
            )}
            onClick={onClick}
        >
            <Icon size={14} className={cn('shrink-0', tone === 'danger' ? 'text-danger' : 'text-text-secondary')} />
            {label}
        </button>
    </PopoverClose>
);
