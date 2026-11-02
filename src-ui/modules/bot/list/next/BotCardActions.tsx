// 两种布局消费同一动作模型；这里不订阅业务状态，也不决定动作能否执行。
import { Fragment } from 'react';
import {
    Activity,
    Copy,
    FileText,
    FlaskConical,
    Globe,
    Monitor,
    MoreHorizontal,
    Play,
    QrCode,
    RefreshCw,
    Server,
    Settings,
    Square,
    SquareTerminal,
    type LucideIcon,
} from 'lucide-react';
import {
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuLabel,
    ContextMenuSeparator,
    Popover,
    PopoverTrigger,
    PopoverContent,
} from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import {
    toolbarActions,
    type BotCardAction,
    type BotCardActionId,
} from '../../../../core/domain/bot/card-actions';
import { IconButton, ToolbarMotionIcon } from './botCardParts';

const ICONS: Record<BotCardActionId, LucideIcon> = {
    start: Play,
    stop: Square,
    webui: Globe,
    novnc: Monitor,
    qr: QrCode,
    remoteQr: QrCode,
    retryUi: RefreshCw,
    configure: Settings,
    migrate: Server,
    logs: FileText,
    metrics: Activity,
    debug: FlaskConical,
    terminal: SquareTerminal,
    hostTerminal: SquareTerminal,
    copy: Copy,
};
interface ActionViewProps {
    actions: readonly BotCardAction[];
    onAction: (id: BotCardActionId) => void;
}

export function BotCardToolbar({
    actions,
    onAction,
    batchMode,
}: ActionViewProps & { batchMode: boolean }) {
    if (batchMode) return <span className="text-2xs text-text-tertiary">点击卡片选择</span>;
    return (
        <>
            {toolbarActions(actions, 'primary').map((action) => (
                <IconButton
                    key={action.id}
                    visible
                    tooltip={action.toolbar?.label ?? action.label}
                    tone={action.tone}
                    disabled={action.disabled}
                    onClick={(event) => {
                        event.stopPropagation();
                        onAction(action.id);
                    }}
                >
                    {action.pending ? (
                        <ActionMotionIcon
                            icon={RefreshCw}
                            size={14}
                            strokeWidth={2.4}
                            motion="spin"
                        />
                    ) : action.id === 'stop' ? (
                        <ActionMotionIcon icon={Square} size={13} strokeWidth={2.6} motion="none" />
                    ) : (
                        <ToolbarMotionIcon
                            icon={ICONS[action.id]}
                            size={action.id === 'qr' ? 15 : 14}
                            strokeWidth={2.2}
                            hoverAccent
                        />
                    )}
                </IconButton>
            ))}
            <Popover>
                <PopoverTrigger asChild>
                    <button
                        type="button"
                        title="更多工具"
                        onClick={(event) => event.stopPropagation()}
                        className="inline-flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                    >
                        <MoreHorizontal size={14} />
                    </button>
                </PopoverTrigger>
                <PopoverContent align="end" className="w-40 p-1">
                    <div className="flex flex-col gap-0.5 text-xs">
                        {toolbarActions(actions, 'more').map((action) => {
                            const Icon = ICONS[action.id];
                            return (
                                <button
                                    key={action.id}
                                    type="button"
                                    disabled={action.disabled}
                                    className="flex items-center gap-2 rounded-xs px-2 py-1.5 text-left text-text-secondary transition-colors hover:bg-inset hover:text-text"
                                    onClick={(event) => {
                                        event.stopPropagation();
                                        onAction(action.id);
                                    }}
                                >
                                    <Icon size={13} className="text-text-tertiary" />
                                    <span>{action.toolbar?.label ?? action.label}</span>
                                </button>
                            );
                        })}
                    </div>
                </PopoverContent>
            </Popover>
        </>
    );
}

export function BotCardContextMenu({
    actions,
    onAction,
    label,
}: ActionViewProps & { label: string }) {
    return (
        <ContextMenuContent className="w-52">
            <ContextMenuLabel className="font-mono text-2xs truncate">{label}</ContextMenuLabel>
            {actions.map((action, index) => {
                const Icon = action.id === 'metrics' ? Monitor : ICONS[action.id];
                const tone =
                    action.id === 'qr'
                        ? undefined
                        : action.tone === 'success'
                          ? 'brand'
                          : action.tone;
                return (
                    <Fragment key={action.id}>
                        {(index === 0 || action.group !== actions[index - 1].group) && (
                            <ContextMenuSeparator />
                        )}
                        <ContextMenuItem
                            tone={action.id === 'start' || action.id === 'stop' ? tone : undefined}
                            disabled={action.disabled}
                            onClick={() => onAction(action.id)}
                        >
                            <Icon
                                size={13}
                                className={
                                    tone === 'brand'
                                        ? 'text-brand'
                                        : tone === 'danger'
                                          ? 'text-danger'
                                          : undefined
                                }
                            />
                            <span>{action.label}</span>
                        </ContextMenuItem>
                    </Fragment>
                );
            })}
        </ContextMenuContent>
    );
}
