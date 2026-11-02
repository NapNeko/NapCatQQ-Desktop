// 列表操作工具栏与右键菜单；保持两种入口的可用性规则一致。
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
import type { BotActorSnapshot } from '../../../../core/ipc/types';
import type { Flavor } from '../../../../core/domain/bot/flavor';
import type { NapcatWebuiBinding } from '../../../../core/domain/webui/availability';
import { canStartBot, canStopBot } from '../../../../core/domain/bot/status';
import { openTerminal } from '../../../../hooks/terminal/terminalStore';
import { openDebugConsole } from '../../../../hooks/debug/debugNav';
import { IconButton, ToolbarMotionIcon } from './botCardParts';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';

export interface BotCardActionProps {
    botId: string;
    state: BotActorSnapshot['state'];
    displayName: string;
    flavor: Flavor | null;
    napcatBinding?: NapcatWebuiBinding | null;
    isBatchMode: boolean;
    isActive: boolean;
    isDockerBot: boolean;
    hasQrcode: boolean;
    startPending: boolean;
    transportFailed: boolean;
    webuiAvailable: boolean;
    webuiTip: string;
    novncAvailable: boolean;
    remoteQrAvailable: boolean;
    snowlumaUiRetryAvailable: boolean;
    onStart: (botId: string) => void;
    onStop: (botId: string) => void;
    onConfigure: (botId: string) => void;
    onViewLogs: (botId: string) => void;
    onViewMetrics: (botId: string) => void;
    onOpenWebui: (params: {
        botId: string;
        flavor: Flavor | null;
        napcat: NapcatWebuiBinding | null;
    }) => void;
    onOpenNovnc?: (botId: string) => void;
    onRetrySnowlumaUi?: (botId: string) => Promise<void>;
    onOpenQr: () => void;
    onOpenRemoteQr: () => void;
    onOpenMigration: () => void;
}

export interface ActionState extends BotCardActionProps {
    terminalEnabled: boolean;
    debugEnabled: boolean;
    retryingSnowlumaUi: boolean;
    runSnowlumaUiRetry: () => Promise<void>;
}

function stopAction(fn: () => void) {
    return (event: React.MouseEvent) => {
        event.stopPropagation();
        fn();
    };
}

export function BotCardToolbar({
    botId,
    state,
    flavor,
    napcatBinding,
    isBatchMode,
    isActive,
    isDockerBot,
    hasQrcode,
    startPending,
    transportFailed,
    webuiAvailable,
    webuiTip,
    novncAvailable,
    remoteQrAvailable,
    snowlumaUiRetryAvailable,
    onStart,
    onStop,
    onConfigure,
    onViewLogs,
    onViewMetrics,
    onOpenWebui,
    onOpenNovnc,
    onRetrySnowlumaUi,
    onOpenQr,
    onOpenRemoteQr,
    onOpenMigration,
    terminalEnabled,
    debugEnabled,
    retryingSnowlumaUi,
    runSnowlumaUiRetry,
}: ActionState) {
    return isBatchMode ? (
        <span className="text-2xs text-text-tertiary">点击卡片选择</span>
    ) : (
        <>
            {/* 扫码登录提示 */}
            {hasQrcode ? (
                <IconButton
                    visible={true}
                    tooltip="扫码登录"
                    onClick={() => onOpenQr()}
                    tone="brand"
                >
                    <ToolbarMotionIcon icon={QrCode} size={15} strokeWidth={2.2} hoverAccent />
                </IconButton>
            ) : null}

            {/* 主操作：启动或停止 */}
            {!isActive && canStartBot(state) ? (
                <IconButton
                    visible={true}
                    tooltip={
                        startPending
                            ? '正在准备启动'
                            : transportFailed
                              ? '远端主机不可达，无法启动'
                              : '启动 Bot'
                    }
                    onClick={stopAction(() => onStart(botId))}
                    disabled={!canStartBot(state) || transportFailed || startPending}
                    tone="success"
                >
                    {startPending ? (
                        <ActionMotionIcon
                            icon={RefreshCw}
                            size={14}
                            strokeWidth={2.4}
                            motion="spin"
                        />
                    ) : (
                        <ToolbarMotionIcon icon={Play} size={14} strokeWidth={2.2} hoverAccent />
                    )}
                </IconButton>
            ) : isActive ? (
                <IconButton
                    visible={true}
                    tooltip="停止 Bot"
                    onClick={stopAction(() => onStop(botId))}
                    disabled={!canStopBot(state)}
                    tone="danger"
                >
                    <ActionMotionIcon icon={Square} size={13} strokeWidth={2.6} motion="none" />
                </IconButton>
            ) : null}

            {/* WebUI 快速入口 */}
            {webuiAvailable ? (
                <IconButton
                    visible={true}
                    tooltip={webuiTip}
                    onClick={stopAction(() =>
                        onOpenWebui({
                            botId: botId,
                            flavor,
                            napcat: napcatBinding ?? null,
                        }),
                    )}
                    tone="brand"
                >
                    <ToolbarMotionIcon icon={Globe} size={14} strokeWidth={2.2} hoverAccent />
                </IconButton>
            ) : null}

            {/* 日志 */}
            <IconButton
                visible={true}
                tooltip="查看日志"
                onClick={stopAction(() => onViewLogs(botId))}
            >
                <ToolbarMotionIcon icon={FileText} size={14} strokeWidth={2.2} hoverAccent />
            </IconButton>

            {/* 配置 */}
            <IconButton
                visible={true}
                tooltip="配置参数"
                onClick={stopAction(() => onConfigure(botId))}
            >
                <ToolbarMotionIcon icon={Settings} size={14} strokeWidth={2.2} hoverAccent />
            </IconButton>

            {/* 次要工具折叠菜单 */}
            <Popover>
                <PopoverTrigger asChild>
                    <button
                        type="button"
                        title="更多工具"
                        onClick={(e) => e.stopPropagation()}
                        className="inline-flex h-7 w-7 items-center justify-center rounded-sm text-text-tertiary transition-colors hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                    >
                        <MoreHorizontal size={14} />
                    </button>
                </PopoverTrigger>
                <PopoverContent align="end" className="w-40 p-1">
                    <div className="flex flex-col gap-0.5 text-xs">
                        <button
                            type="button"
                            className="flex items-center gap-2 rounded-xs px-2 py-1.5 text-left text-text-secondary transition-colors hover:bg-inset hover:text-text"
                            onClick={stopAction(() => onOpenMigration())}
                        >
                            <Server size={13} className="text-text-tertiary" />
                            <span>迁移到服务器</span>
                        </button>
                        <button
                            type="button"
                            className="flex items-center gap-2 rounded-xs px-2 py-1.5 text-left text-text-secondary transition-colors hover:bg-inset hover:text-text"
                            onClick={stopAction(() => onViewMetrics(botId))}
                        >
                            <Activity size={13} className="text-text-tertiary" />
                            <span>运行监控</span>
                        </button>
                        {terminalEnabled ? (
                            <button
                                type="button"
                                className="flex items-center gap-2 rounded-xs px-2 py-1.5 text-left text-text-secondary transition-colors hover:bg-inset hover:text-text"
                                onClick={stopAction(
                                    () =>
                                        void openTerminal({
                                            kind: 'bot',
                                            bot_id: botId,
                                            host_dir: false,
                                        }),
                                )}
                            >
                                <SquareTerminal size={13} className="text-text-tertiary" />
                                <span>{isDockerBot ? '终端（容器内）' : '打开终端'}</span>
                            </button>
                        ) : null}
                        {novncAvailable && onOpenNovnc ? (
                            <button
                                type="button"
                                className="flex items-center gap-2 rounded-xs px-2 py-1.5 text-left text-text-secondary transition-colors hover:bg-inset hover:text-text"
                                onClick={stopAction(() => onOpenNovnc(botId))}
                            >
                                <Monitor size={13} className="text-text-tertiary" />
                                <span>noVNC 桌面</span>
                            </button>
                        ) : null}
                        {remoteQrAvailable ? (
                            <button
                                type="button"
                                className="flex items-center gap-2 rounded-xs px-2 py-1.5 text-left text-text-secondary transition-colors hover:bg-inset hover:text-text"
                                onClick={stopAction(() => onOpenRemoteQr())}
                            >
                                <QrCode size={13} className="text-text-tertiary" />
                                <span>扫码登录</span>
                            </button>
                        ) : null}
                        {debugEnabled && state === 'running' ? (
                            <button
                                type="button"
                                className="flex items-center gap-2 rounded-xs px-2 py-1.5 text-left text-text-secondary transition-colors hover:bg-inset hover:text-text"
                                onClick={stopAction(() => openDebugConsole(botId))}
                            >
                                <FlaskConical size={13} className="text-text-tertiary" />
                                <span>调试控制台</span>
                            </button>
                        ) : null}
                        {snowlumaUiRetryAvailable && onRetrySnowlumaUi ? (
                            <button
                                type="button"
                                className="flex items-center gap-2 rounded-xs px-2 py-1.5 text-left text-text-secondary transition-colors hover:bg-inset hover:text-text"
                                disabled={retryingSnowlumaUi}
                                onClick={stopAction(() => void runSnowlumaUiRetry())}
                            >
                                <RefreshCw size={13} className="text-text-tertiary" />
                                <span>重建连接</span>
                            </button>
                        ) : null}
                    </div>
                </PopoverContent>
            </Popover>
        </>
    );
}

export function BotCardContextMenu({
    botId,
    state,
    displayName,
    flavor,
    napcatBinding,
    isActive,
    isDockerBot,
    hasQrcode,
    startPending,
    transportFailed,
    webuiAvailable,
    novncAvailable,
    remoteQrAvailable,
    snowlumaUiRetryAvailable,
    onStart,
    onStop,
    onConfigure,
    onViewLogs,
    onViewMetrics,
    onOpenWebui,
    onOpenNovnc,
    onRetrySnowlumaUi,
    onOpenQr,
    onOpenRemoteQr,
    onOpenMigration,
    terminalEnabled,
    debugEnabled,
    retryingSnowlumaUi,
    runSnowlumaUiRetry,
}: ActionState) {
    const handleCopyQqid = async () => {
        try {
            await navigator.clipboard.writeText(botId);
            pushInfoBar({
                tone: 'info',
                title: '已复制 QQ 号',
                content: botId,
                autoDismissMs: 2000,
            });
        } catch {
            // ignore
        }
    };
    return (
        <ContextMenuContent className="w-52">
            <ContextMenuLabel className="font-mono text-2xs truncate">
                QQ {botId} {displayName !== botId ? `(${displayName})` : ''}
            </ContextMenuLabel>
            <ContextMenuSeparator />

            {/* 运行控制 */}
            {!isActive && canStartBot(state) && (
                <ContextMenuItem
                    tone="brand"
                    disabled={startPending || transportFailed}
                    onClick={() => onStart(botId)}
                >
                    <Play size={13} className="text-brand" />
                    <span>启动实例</span>
                </ContextMenuItem>
            )}
            {isActive && (
                <ContextMenuItem
                    tone="danger"
                    disabled={!canStopBot(state)}
                    onClick={() => onStop(botId)}
                >
                    <Square size={13} className="text-danger" />
                    <span>停止实例</span>
                </ContextMenuItem>
            )}
            {webuiAvailable && (
                <ContextMenuItem
                    onClick={() =>
                        onOpenWebui({
                            botId: botId,
                            flavor,
                            napcat: napcatBinding ?? null,
                        })
                    }
                >
                    <Globe size={13} className="text-brand" />
                    <span>打开 WebUI</span>
                </ContextMenuItem>
            )}
            {novncAvailable && onOpenNovnc && (
                <ContextMenuItem onClick={() => onOpenNovnc(botId)}>
                    <Monitor size={13} className="text-brand" />
                    <span>打开 noVNC 桌面</span>
                </ContextMenuItem>
            )}
            {remoteQrAvailable && (
                <ContextMenuItem onClick={() => onOpenRemoteQr()}>
                    <QrCode size={13} className="text-brand" />
                    <span>扫码登录</span>
                </ContextMenuItem>
            )}
            {snowlumaUiRetryAvailable && onRetrySnowlumaUi && (
                <ContextMenuItem
                    disabled={retryingSnowlumaUi}
                    onClick={() => void runSnowlumaUiRetry()}
                >
                    <RefreshCw size={13} className="text-brand" />
                    <span>重建 WebUI 连接</span>
                </ContextMenuItem>
            )}
            {hasQrcode && (
                <ContextMenuItem onClick={() => onOpenQr()}>
                    <QrCode size={13} />
                    <span>扫码登录</span>
                </ContextMenuItem>
            )}

            <ContextMenuSeparator />

            {/* 导航跳转 */}
            <ContextMenuItem onClick={() => onConfigure(botId)}>
                <Settings size={13} />
                <span>配置参数</span>
            </ContextMenuItem>
            <ContextMenuItem onClick={() => onOpenMigration()}>
                <Server size={13} />
                <span>迁移到服务器</span>
            </ContextMenuItem>
            <ContextMenuItem onClick={() => onViewLogs(botId)}>
                <FileText size={13} />
                <span>查看日志</span>
            </ContextMenuItem>
            <ContextMenuItem onClick={() => onViewMetrics(botId)}>
                <Monitor size={13} />
                <span>运行监控</span>
            </ContextMenuItem>
            {debugEnabled && (
                <ContextMenuItem onClick={() => openDebugConsole(botId)}>
                    <FlaskConical size={13} />
                    <span>在调试台打开</span>
                </ContextMenuItem>
            )}
            {terminalEnabled && (
                <ContextMenuItem
                    onClick={() =>
                        void openTerminal({ kind: 'bot', bot_id: botId, host_dir: false })
                    }
                >
                    <SquareTerminal size={13} />
                    <span>{isDockerBot ? '终端（进容器）' : '打开终端'}</span>
                </ContextMenuItem>
            )}
            {terminalEnabled && isDockerBot && (
                <ContextMenuItem
                    onClick={() =>
                        void openTerminal({ kind: 'bot', bot_id: botId, host_dir: true })
                    }
                >
                    <SquareTerminal size={13} />
                    <span>终端（宿主机部署目录）</span>
                </ContextMenuItem>
            )}

            <ContextMenuSeparator />

            {/* 快捷复制 */}
            <ContextMenuItem onClick={handleCopyQqid}>
                <Copy size={13} />
                <span>复制 QQ 号</span>
            </ContextMenuItem>
        </ContextMenuContent>
    );
}
