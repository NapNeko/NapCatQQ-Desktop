// 自绘托盘面板:右键托盘图标弹出的操作卡片。
// 支持在轻量模式下直接管理/启停 Bot、打开 WebUI / noVNC 与查看运行状态。

import React, { useEffect, useRef, useState, useCallback } from 'react';
import {
    BatteryCharging,
    ChevronLeft,
    ChevronRight,
    Globe,
    Loader2,
    LogOut,
    Monitor,
    PanelsTopLeft,
    Play,
    Square,
} from 'lucide-react';
import { botService } from '../../core/services/bot.service';
import { trayService } from '../../core/services/desktop.service';
import { useBotSnapshots } from '../../hooks/bot/useBotSnapshots';
import { useBotConfigsMap } from '../../hooks/bot/useBotConfigsMap';
import { useBotFlavorMap } from '../../hooks/bot/useBotFlavorMap';
import { useSortedBots, botSortStore } from '../../hooks/bot/useBotSort';
import { useNapcatLogin } from '../../hooks/webui/useNapcatLogin';
import { useOpenWebui } from '../../hooks/webui/useOpenWebui';
import { useOpenSnowlumaNovnc } from '../../hooks/webui/useOpenSnowlumaNovnc';
import type { BotActorSnapshot } from '../../core/ipc/types';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import type { Flavor } from '../../core/domain/bot/flavor';
import { cn } from '../../shared/utils/cn';
import logoMark from '../../assets/logo.png';
const PAGE_SIZE = 2;
let lastReportedHeight = 0;

// 高度自适应:量取 cardRef 自然内容高度并通知 Rust 动态调窗
async function reportPanelHeight(el: HTMLElement): Promise<void> {
    try {
        const { invoke } = await import('../../core/ipc/transport');
        const h = Math.ceil(el.getBoundingClientRect().height);
        if (h > 60 && h < 600 && Math.abs(h - lastReportedHeight) >= 2) {
            lastReportedHeight = h;
            await invoke<void>('tray_panel_resize', { height: h });
        }
    } catch {
        // 浏览器预览下没有 IPC,忽略
    }
}

async function hidePanel(): Promise<void> {
    try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        await getCurrentWindow().hide();
    } catch {
        // 浏览器预览下没有 window API,忽略
    }
}

interface PanelActionProps {
    icon: React.ReactNode;
    title: string;
    danger?: boolean;
    onClick: () => void | Promise<void>;
}

function PanelAction({ icon, title, danger, onClick }: PanelActionProps) {
    return (
        <button
            type="button"
            onClick={() => void onClick()}
            className={[
                'group flex w-full items-center gap-2.5 rounded-md px-2.5 py-[6px] text-left cursor-pointer',
                'transition-colors duration-100',
                danger ? 'hover:bg-danger-soft' : 'hover:bg-brand-soft',
            ].join(' ')}
        >
            <span
                className={[
                    'flex shrink-0 items-center justify-center transition-colors',
                    danger ? 'text-danger' : 'text-text-secondary group-hover:text-brand',
                ].join(' ')}
            >
                {icon}
            </span>
            <span
                className={[
                    'min-w-0 flex-1 truncate text-[12.5px] leading-none',
                    danger ? 'text-danger' : 'text-text group-hover:text-brand',
                ].join(' ')}
            >
                {title}
            </span>
        </button>
    );
}

interface TrayBotItemProps {
    snapshot: BotActorSnapshot;
    config?: BotConfig | null;
    flavor?: Flavor;
    onStart: (botId: string) => Promise<void>;
    onStop: (botId: string) => Promise<void>;
    onOpenWebui: (botId: string, flavor?: Flavor) => Promise<void>;
    onOpenNovnc: (botId: string) => Promise<void>;
    isMutating: boolean;
}

const AVATAR_PALETTES = [
    'from-pink-300 to-rose-400',
    'from-amber-300 to-orange-400',
    'from-emerald-300 to-teal-400',
    'from-sky-300 to-indigo-400',
    'from-violet-300 to-fuchsia-400',
    'from-rose-300 to-red-400',
] as const;

function pickAvatarPalette(seed: string): string {
    let h = 0;
    for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) | 0;
    return AVATAR_PALETTES[Math.abs(h) % AVATAR_PALETTES.length] ?? AVATAR_PALETTES[0];
}

function TrayBotAvatar({
    qqid,
    displayName,
}: {
    qqid: string;
    displayName: string;
}) {
    const [failed, setFailed] = useState(false);
    const numericQQ = /^\d+$/.test(qqid) ? qqid : null;
    const showImg = numericQQ && !failed;
    const initials = (displayName.trim().charAt(0) || '?').toUpperCase();
    const palette = pickAvatarPalette(qqid);

    return (
        <div className="relative h-7.5 w-7.5 shrink-0 overflow-hidden rounded-md ring-1 ring-border-subtle/70">
            <div className={cn('h-full w-full overflow-hidden rounded-md bg-gradient-to-br', palette)}>
                {showImg ? (
                    <img
                        src={`https://q.qlogo.cn/headimg_dl?dst_uin=${numericQQ}&spec=100`}
                        alt=""
                        className="h-full w-full object-cover"
                        referrerPolicy="no-referrer"
                        draggable={false}
                        onError={() => setFailed(true)}
                    />
                ) : (
                    <div className="flex h-full w-full items-center justify-center text-[11px] font-semibold text-white/95">
                        {initials}
                    </div>
                )}
            </div>
        </div>
    );
}

function TrayBotItem({
    snapshot,
    config,
    flavor,
    onStart,
    onStop,
    onOpenWebui,
    onOpenNovnc,
    isMutating,
}: TrayBotItemProps) {
    const isRunning = snapshot.state === 'running';
    const isTransitioning =
        snapshot.state === 'starting' ||
        snapshot.state === 'stopping' ||
        isMutating;
    const isError = snapshot.state === 'crashed';
    const botName = config?.bot.name?.trim() || snapshot.bot_id;
    const qqId = config?.bot.QQID ? String(config.bot.QQID) : snapshot.bot_id;
    const isSL = flavor === 'snowluma' || config?.bot.backend_type === 'snowluma';

    const statusTitle = isRunning
        ? '运行中'
        : isTransitioning
            ? '切换中...'
            : isError
                ? '异常退出'
                : '已停止';

    return (
        <div className="flex items-center justify-between gap-2 rounded-md bg-surface-muted/60 px-2 py-1.5 ring-1 ring-border-subtle/50 transition-colors hover:bg-surface-muted">
            {/* 左侧：Bot 头像 + 名字 + QQ */}
            <div className="flex min-w-0 flex-1 items-center gap-2">
                <TrayBotAvatar qqid={qqId} displayName={botName} />
                <div className="flex min-w-0 flex-1 flex-col justify-center">
                    <span className="truncate text-[12px] font-medium leading-tight text-text">
                        {botName}
                    </span>
                    <span className="truncate font-mono text-[10.5px] leading-none text-text-tertiary">
                        QQ: {qqId}
                    </span>
                </div>
            </div>

            {/* 右侧：状态指示圆点 + 快捷操作按钮组 */}
            <div className="flex shrink-0 items-center gap-1">
                {/* 状态指示小圆点 */}
                <span
                    title={statusTitle}
                    className={cn(
                        'h-2 w-2 shrink-0 rounded-full mr-0.5',
                        isRunning
                            ? 'bg-success'
                            : isTransitioning
                                ? 'bg-brand animate-pulse'
                                : isError
                                    ? 'bg-danger'
                                    : 'bg-text-disabled',
                    )}
                />
                {/* noVNC 远程桌面/扫码按钮 (SnowLuma 且运行中时显示) */}
                {isRunning && isSL && (
                    <button
                        type="button"
                        onClick={(e) => {
                            e.stopPropagation();
                            void onOpenNovnc(snapshot.bot_id);
                        }}
                        title="打开 noVNC 扫码 / 远程桌面"
                        className="flex h-6 w-6 items-center justify-center rounded text-text-secondary transition-colors hover:bg-brand-soft hover:text-brand cursor-pointer"
                    >
                        <Monitor size={13} strokeWidth={1.9} />
                    </button>
                )}

                {/* WebUI 按钮 (运行中时可用) */}
                {isRunning && (
                    <button
                        type="button"
                        onClick={(e) => {
                            e.stopPropagation();
                            void onOpenWebui(snapshot.bot_id, flavor);
                        }}
                        title="打开 WebUI 控制台"
                        className="flex h-6 w-6 items-center justify-center rounded text-text-secondary transition-colors hover:bg-brand-soft hover:text-brand cursor-pointer"
                    >
                        <Globe size={13} strokeWidth={1.9} />
                    </button>
                )}

                {/* 启动 / 停止 按钮 */}
                {isTransitioning ? (
                    <span className="flex h-6 w-6 items-center justify-center text-brand">
                        <Loader2 size={13} className="animate-spin" />
                    </span>
                ) : isRunning ? (
                    <button
                        type="button"
                        onClick={(e) => {
                            e.stopPropagation();
                            void onStop(snapshot.bot_id);
                        }}
                        title="停止 Bot"
                        className="flex h-6 w-6 items-center justify-center rounded text-text-secondary transition-colors hover:bg-danger-soft hover:text-danger cursor-pointer"
                    >
                        <Square size={11} strokeWidth={2} fill="currentColor" />
                    </button>
                ) : (
                    <button
                        type="button"
                        onClick={(e) => {
                            e.stopPropagation();
                            void onStart(snapshot.bot_id);
                        }}
                        title="启动 Bot"
                        className="flex h-6 w-6 items-center justify-center rounded text-text-secondary transition-colors hover:bg-brand-soft hover:text-brand cursor-pointer"
                    >
                        <Play size={11} strokeWidth={2} fill="currentColor" />
                    </button>
                )}
            </div>
        </div>
    );
}

export const TrayPanel: React.FC = () => {
    const cardRef = useRef<HTMLDivElement>(null);
    const { data: rawBotSnapshots = [], refetch } = useBotSnapshots({
        disablePolling: true,
    });
    const configByBot = useBotConfigsMap(rawBotSnapshots);
    const flavorByBot = useBotFlavorMap(rawBotSnapshots);
    const { sortedBots: botSnapshots } = useSortedBots(rawBotSnapshots);
    const napcat = useNapcatLogin();
    const openWebui = useOpenWebui();
    const openSnowlumaNovnc = useOpenSnowlumaNovnc();
    const [mutatingBotIds, setMutatingBotIds] = useState<Record<string, boolean>>({});
    const [actionError, setActionError] = useState<string | null>(null);
    const [page, setPage] = useState(1);

    const runningCount = botSnapshots.filter((s) => s.state === 'running').length;
    const totalCount = botSnapshots.length;
    const totalPages = Math.max(1, Math.ceil(totalCount / PAGE_SIZE));
    const currentPage = Math.min(page, totalPages);

    const visibleBots = botSnapshots.slice(
        (currentPage - 1) * PAGE_SIZE,
        currentPage * PAGE_SIZE,
    );

    const handleStart = async (botId: string) => {
        setActionError(null);
        setMutatingBotIds((prev) => ({ ...prev, [botId]: true }));
        try {
            await botService.start(botId);
        } catch (e) {
            console.error('启动 Bot 失败:', e);
            setActionError('启动失败，详情见日志');
        } finally {
            setMutatingBotIds((prev) => ({ ...prev, [botId]: false }));
            void refetch();
        }
    };

    const handleStop = async (botId: string) => {
        setActionError(null);
        setMutatingBotIds((prev) => ({ ...prev, [botId]: true }));
        try {
            await botService.stop(botId);
        } catch (e) {
            console.error('停止 Bot 失败:', e);
            setActionError('停止失败，详情见日志');
        } finally {
            setMutatingBotIds((prev) => ({ ...prev, [botId]: false }));
            void refetch();
        }
    };

    const handleOpenWebui = async (botId: string, flavor?: Flavor) => {
        try {
            await openWebui({
                botId,
                flavor: flavor ?? flavorByBot[botId],
                napcat: napcat.byBot[botId]?.webui,
            });
        } catch (e) {
            console.error('打开 WebUI 失败:', e);
            setActionError('打开失败，详情见日志');
        }
    };

    const handleOpenNovnc = async (botId: string) => {
        try {
            await openSnowlumaNovnc(botId);
        } catch (e) {
            console.error('打开 noVNC 失败:', e);
            setActionError('打开失败，详情见日志');
        }
    };

    const refreshAndResize = useCallback(() => {
        setActionError(null);
        botSortStore.refresh();
        void refetch();
        const el = cardRef.current;
        if (el) {
            void reportPanelHeight(el);
        }
    }, [refetch]);

    useEffect(() => {
        refreshAndResize();
    }, [refreshAndResize]);

    // 监听托盘唤起事件:每次展开时实时刷新 Bot 排序、运行状态并校准高度
    useEffect(() => {
        let unlisten: (() => void) | undefined;
        const setup = async () => {
            try {
                const { getCurrentWindow } = await import('@tauri-apps/api/window');
                unlisten = await getCurrentWindow().listen('tray_panel_show', () => {
                    refreshAndResize();
                });
            } catch {
                // 忽略非 Tauri 环境
            }
        };
        void setup();
        return () => {
            if (unlisten) unlisten();
        };
    }, [refreshAndResize]);

    // 翻页或当前页 Bot 数量变动时，即时量取新高度并通知 Rust 调窗
    useEffect(() => {
        const el = cardRef.current;
        if (el) {
            void reportPanelHeight(el);
        }
    }, [currentPage, visibleBots.length]);

    // 监听内容高度变化并实时向 Rust 上报调窗
    useEffect(() => {
        const el = cardRef.current;
        if (!el || typeof ResizeObserver === 'undefined') return;
        const ro = new ResizeObserver(() => {
            void reportPanelHeight(el);
        });
        ro.observe(el);
        return () => {
            ro.disconnect();
        };
    }, []);

    const statusText =
        totalCount === 0
            ? '后台待命'
            : runningCount > 0
                ? `${runningCount}/${totalCount} 个 Bot 运行中`
                : '全部已停止';

    const handleShow = async () => {
        await hidePanel();
        await trayService.showMainWindow().catch(() => { });
    };

    const handleLightweight = async () => {
        const { invoke } = await import('../../core/ipc/transport');
        await invoke<void>('tray_panel_enter_lightweight').catch(() => { });
    };

    const handleQuit = async () => {
        const { invoke } = await import('../../core/ipc/transport');
        await invoke<void>('tray_panel_quit').catch(() => { });
    };

    return (
        <div className="flex min-h-full w-full flex-col overflow-hidden bg-elevated select-none">
            <div
                ref={cardRef}
                role="menu"
                aria-label="NapCatQQ Desktop 托盘菜单"
                className="flex w-full flex-col"
            >
                {/* 头部:logo + 产品名,状态点带运行摘要（无右侧停止按钮） */}
                <div className="flex items-center gap-2.5 px-3 pb-1 pt-2.5">
                    <img
                        src={logoMark}
                        alt="NapCatQQ-Desktop logo"
                        width={24}
                        height={24}
                        className="h-6 w-6 shrink-0 rounded-md object-cover ring-1 ring-border-subtle/60 [image-rendering:-webkit-optimize-contrast]"
                        draggable={false}
                    />
                    <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                        <span className="truncate whitespace-nowrap text-[12.5px] font-semibold leading-none text-text">
                            NapCatQQ-Desktop
                        </span>
                        <span className="flex items-center gap-1.5">
                            <span
                                className={[
                                    'h-1.5 w-1.5 shrink-0 rounded-full',
                                    runningCount > 0
                                        ? 'bg-success'
                                        : 'bg-text-disabled',
                                ].join(' ')}
                            />
                            <span className="truncate text-[11px] leading-none text-text-tertiary">
                                {statusText}
                            </span>
                        </span>
                    </div>
                </div>

                {actionError ? (
                    <div
                        role="alert"
                        className="mx-2 mb-1 rounded-md bg-danger-soft px-2 py-1 text-[11px] leading-snug text-danger"
                    >
                        {actionError}
                    </div>
                ) : null}

                {/* Bot 快捷卡片列表 (单页最多展示 2 个，超出提供翻页) */}
                {totalCount > 0 && (
                    <>
                        <div className="mx-2 my-1 border-t border-border-subtle" />

                        {/* 超出 2 个时渲染紧凑翻页工具栏 */}
                        {totalPages > 1 && (
                            <div className="flex items-center justify-between px-2.5 pb-1 text-[11px] text-text-tertiary">
                                <span className="truncate">机器人列表 ({totalCount})</span>
                                <div className="flex items-center gap-1.5">
                                    <button
                                        type="button"
                                        disabled={currentPage <= 1}
                                        onClick={() => setPage((p) => Math.max(1, p - 1))}
                                        className="flex h-5 w-5 items-center justify-center rounded text-text-secondary transition-colors hover:bg-brand-soft hover:text-brand disabled:opacity-30 disabled:hover:bg-transparent disabled:hover:text-inherit cursor-pointer disabled:cursor-not-allowed"
                                        title="上一页"
                                    >
                                        <ChevronLeft size={13} />
                                    </button>
                                    <span className="font-mono text-[11px] leading-none text-text-secondary">
                                        {currentPage} / {totalPages}
                                    </span>
                                    <button
                                        type="button"
                                        disabled={currentPage >= totalPages}
                                        onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
                                        className="flex h-5 w-5 items-center justify-center rounded text-text-secondary transition-colors hover:bg-brand-soft hover:text-brand disabled:opacity-30 disabled:hover:bg-transparent disabled:hover:text-inherit cursor-pointer disabled:cursor-not-allowed"
                                        title="下一页"
                                    >
                                        <ChevronRight size={13} />
                                    </button>
                                </div>
                            </div>
                        )}

                        <div className="flex flex-col gap-1 px-2 py-0.5">
                            {visibleBots.map((snap) => (
                                <TrayBotItem
                                    key={snap.bot_id}
                                    snapshot={snap}
                                    config={configByBot[snap.bot_id]}
                                    flavor={flavorByBot[snap.bot_id]}
                                    onStart={handleStart}
                                    onStop={handleStop}
                                    onOpenWebui={handleOpenWebui}
                                    onOpenNovnc={handleOpenNovnc}
                                    isMutating={!!mutatingBotIds[snap.bot_id]}
                                />
                            ))}
                        </div>
                    </>
                )}

                <div className="mx-2 my-0.5 border-t border-border-subtle" />

                {/* 全局操作:显示主窗口与释放内存 */}
                <div className="flex flex-col px-1 py-0.5">
                    <PanelAction
                        icon={<PanelsTopLeft size={14} strokeWidth={1.9} />}
                        title="显示主窗口"
                        onClick={handleShow}
                    />
                    <PanelAction
                        icon={<BatteryCharging size={14} strokeWidth={1.9} />}
                        title="释放界面内存"
                        onClick={handleLightweight}
                    />
                </div>

                <div className="mx-2 my-0.5 border-t border-border-subtle" />

                {/* 退出 */}
                <div className="flex flex-col px-1 pb-1 pt-0.5">
                    <PanelAction
                        icon={<LogOut size={14} strokeWidth={1.9} />}
                        title="退出"
                        onClick={handleQuit}
                    />
                </div>
            </div>
        </div>
    );
};

export default TrayPanel;
