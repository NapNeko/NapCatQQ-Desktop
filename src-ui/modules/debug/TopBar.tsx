// 调试台顶栏：选「发给谁」（Bot）、走哪条通道、事件从哪来，外加后台接收状态、命令面板入口、右栏开关。
//
// 标签页决定「发什么」，不在这里。窄窗口时靠容器查询把次要的字收掉（QQ 号、「正在接收」的长文案、
// 命令面板按钮上的字），按钮本身都在。

import { memo } from 'react';
import { PanelRightClose, PanelRightOpen, Search } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../shared/ui';
import { ActionMotionIcon } from '../../shared/ui/motion';
import type { AppRoute } from '../../shared/components/next/Sidebar';
import type { DebugChannelChoice } from '../../core/ipc/generated/debug/DebugChannelChoice';
import type { DebugChannels } from '../../core/ipc/generated/debug/DebugChannels';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { BotPicker } from './BotPicker';
import { ChannelSelect } from './ChannelSelect';
import { ReceivingIndicator } from './ReceivingIndicator';

const IS_MAC = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
export const MOD_KEY_LABEL = IS_MAC ? '⌘' : 'Ctrl';

export interface TopBarProps {
    targets: readonly DebugTarget[];
    selected: DebugTarget | null;
    targetsLoading: boolean;
    onSelectBot: (botId: string) => void;
    onManageBots?: () => void;

    channels: DebugChannels | undefined;
    channelsLoading: boolean;
    channelsError: boolean;
    onRetryChannels: () => void;
    choice: DebugChannelChoice;
    onChoiceChange: (choice: DebugChannelChoice) => void;
    /** 调用通道下拉的开关，由页面持有：中栏的「查看通道」要能把它点开 */
    callChannelOpen: boolean;
    onCallChannelOpenChange: (open: boolean) => void;

    rightCollapsed: boolean;
    onToggleRight: () => void;
    onOpenPalette: () => void;
    /** 通道下拉里「去哪解决」的出口按主路由跳；没给就不画 */
    onNavigate?: (route: AppRoute) => void;
}

export const TopBar = memo(function TopBar({
    targets,
    selected,
    targetsLoading,
    onSelectBot,
    onManageBots,
    channels,
    channelsLoading,
    channelsError,
    onRetryChannels,
    choice,
    onChoiceChange,
    callChannelOpen,
    onCallChannelOpenChange,
    rightCollapsed,
    onToggleRight,
    onOpenPalette,
    onNavigate,
}: TopBarProps) {
    const botId = selected?.bot_id ?? null;
    const running = selected?.running ?? false;

    return (
        <header className="@container shrink-0 pt-2">
            <div className="flex min-w-0 items-center gap-3">
                <div className="shrink-0">
                    <p className="text-2xs uppercase leading-none tracking-widest text-text-tertiary">debug</p>
                    <h1 className="mt-1 font-display text-xl font-semibold leading-none text-text">调试台</h1>
                </div>

                <span aria-hidden className="h-8 w-px shrink-0 bg-border-subtle" />

                <div role="group" aria-label="调试目标" className="flex min-w-0 flex-1 items-center gap-1.5">
                    <BotPicker
                        targets={targets}
                        selected={selected}
                        loading={targetsLoading}
                        onSelect={onSelectBot}
                        onManageBots={onManageBots}
                    />
                    <ChannelSelect
                        purpose="call"
                        botId={botId}
                        running={running}
                        channels={channels}
                        loading={channelsLoading}
                        error={channelsError}
                        onRetry={onRetryChannels}
                        value={choice.call}
                        onChange={(call) => onChoiceChange({ ...choice, call })}
                        open={callChannelOpen}
                        onOpenChange={onCallChannelOpenChange}
                        onNavigate={onNavigate}
                    />
                    <ChannelSelect
                        purpose="events"
                        botId={botId}
                        running={running}
                        channels={channels}
                        loading={channelsLoading}
                        error={channelsError}
                        onRetry={onRetryChannels}
                        value={choice.events}
                        onChange={(events) => onChoiceChange({ ...choice, events })}
                        onNavigate={onNavigate}
                    />
                    <span className="min-w-2 flex-1" />
                    <ReceivingIndicator targets={targets} watchBotId={botId} />
                </div>

                <div className="flex shrink-0 items-center gap-1">
                    <button
                        type="button"
                        onClick={onOpenPalette}
                        aria-label={`搜索接口（${MOD_KEY_LABEL}+K）`}
                        className={cn(
                            'inline-flex h-8 items-center gap-2 rounded-sm border border-border-subtle bg-surface px-2 text-[12.5px] text-text-tertiary',
                            'transition-colors hover:border-border hover:bg-inset hover:text-text-secondary',
                            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                        )}
                    >
                        <Search size={13} strokeWidth={2} aria-hidden />
                        <span className="hidden @min-[1040px]:inline">搜索接口</span>
                        <kbd className="rounded-xs border border-border-subtle bg-inset px-1 py-px font-mono text-[10px] leading-none text-text-tertiary">
                            {MOD_KEY_LABEL} K
                        </kbd>
                    </button>
                    <Tooltip>
                        <TooltipTrigger asChild>
                            <button
                                type="button"
                                onClick={onToggleRight}
                                aria-label={rightCollapsed ? '展开右栏（聊天）' : '收起右栏（聊天）'}
                                aria-pressed={!rightCollapsed}
                                className={cn(
                                    'inline-flex h-8 w-8 items-center justify-center rounded-sm transition-colors',
                                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                                    rightCollapsed
                                        ? 'text-text-tertiary hover:bg-inset hover:text-text'
                                        : 'bg-inset text-text-secondary hover:text-text',
                                )}
                            >
                                <ActionMotionIcon icon={rightCollapsed ? PanelRightOpen : PanelRightClose} size={16} strokeWidth={2} />
                            </button>
                        </TooltipTrigger>
                        <TooltipContent side="bottom">{rightCollapsed ? '展开右栏（聊天）' : '收起右栏'}</TooltipContent>
                    </Tooltip>
                </div>
            </div>
        </header>
    );
});
