// 右下角悬浮三圆按钮（新增 / 刷新 / 进入批量模式）。
//
// 设计沿用旧 Python 版 BotListPage 的 floating action group 习惯：用户最常用
// 的三个动作贴在屏幕右下角，不占列表行高，鼠标永远能找到。
//
// 视觉走暖粉桃色调：底层 bg-elevated（暖米黄）+ 暗色 ring；hover 时升起
// shadow-popover；新增按钮用 brand primary 高亮，其它两个走 ghost 调性。
//
// 互斥规则：批量模式开启时本组隐藏，由 BatchBottomBar 接管。

import { Plus, RefreshCw, ListChecks } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';

interface FloatingActionsProps {
    onCreate: () => void;
    onRefresh: () => void;
    onEnterBatch: () => void;
    /** 后端 mutation 进行中：禁用所有按钮避免重复触发。 */
    busy?: boolean;
    /** 批量模式开启时整组隐藏。 */
    hidden?: boolean;
}

export function FloatingActions({
    onCreate,
    onRefresh,
    onEnterBatch,
    busy = false,
    hidden = false,
}: FloatingActionsProps) {
    if (hidden) return null;

    return (
        <div className="pointer-events-none fixed bottom-8 right-8 z-30 flex flex-col items-center gap-3">
            <CircleButton
                tooltip="批量管理"
                onClick={onEnterBatch}
                disabled={busy}
                variant="ghost"
            >
                <ListChecks size={18} strokeWidth={2.2} />
            </CircleButton>
            <CircleButton
                tooltip="刷新列表"
                onClick={onRefresh}
                disabled={busy}
                variant="ghost"
            >
                <RefreshCw size={18} strokeWidth={2.2} />
            </CircleButton>
            <CircleButton
                tooltip="新增 Bot"
                onClick={onCreate}
                disabled={busy}
                variant="primary"
            >
                <Plus size={20} strokeWidth={2.4} />
            </CircleButton>
        </div>
    );
}

interface CircleButtonProps {
    tooltip: string;
    onClick: () => void;
    disabled?: boolean;
    variant: 'primary' | 'ghost';
    children: React.ReactNode;
}

function CircleButton({
    tooltip,
    onClick,
    disabled,
    variant,
    children,
}: CircleButtonProps) {
    return (
        <Tooltip>
            <TooltipTrigger asChild>
                <button
                    type="button"
                    onClick={onClick}
                    disabled={disabled}
                    className={cn(
                        // pointer-events-auto 必须重启，因为父级 fixed 容器禁掉了点击
                        'pointer-events-auto inline-flex h-11 w-11 items-center justify-center rounded-full',
                        'transition-all duration-150',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                        'disabled:cursor-not-allowed disabled:opacity-50',
                        variant === 'primary'
                            ? 'bg-brand text-white shadow-popover hover:bg-brand-hover hover:scale-105'
                            : 'bg-elevated text-text-secondary ring-1 ring-border-subtle shadow-card hover:bg-inset hover:text-text hover:shadow-popover',
                    )}
                >
                    {children}
                </button>
            </TooltipTrigger>
            <TooltipContent side="left">{tooltip}</TooltipContent>
        </Tooltip>
    );
}
