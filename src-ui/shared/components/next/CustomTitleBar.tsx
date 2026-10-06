// 自定义窗口标题栏（next）。
//
// 视觉决策：
//   - 浮动毛玻璃极简胶囊岛（Floating Capsule Island）
//   - 三态微交互：最小化（琥珀金反馈）、最大化/还原（翡翠绿反馈）、关闭（珊瑚红反馈）
//   - 独立圆形微按钮 + active 弹性微缩放反馈
//   - 整条作为 drag region（可拖拽窗口、双击最大化）

import React from 'react';
import { Copy, Minus, Square, X } from 'lucide-react';
import { cn } from '../../utils/cn';
import { useWindowControls } from '../../../hooks/desktop/useWindowControls';
import { windowControlService } from '../../../core/services/desktop.service';
import { useFeatureEnabled } from '../../../hooks/preferences/featureTogglesStore';
import { TerminalToggleButton } from './TerminalToggleButton';

interface CustomTitleBarProps {
    className?: string;
    /**
     * window = 独立工具窗（调试台弹出窗）：不挂终端开关（终端面板在主窗里），
     * 关闭只关自己，不走主窗的托盘隐藏 / 退出闸门。
     */
    variant?: 'app' | 'window';
}

export const CustomTitleBar: React.FC<CustomTitleBarProps> = ({ className, variant = 'app' }) => {
    const { isMaximized, minimize, toggleMaximize, close } = useWindowControls();
    const terminalEnabled = useFeatureEnabled('terminal');
    const isToolWindow = variant === 'window';
    const onClose = isToolWindow ? () => void windowControlService.closeSelf() : close;

    return (
        <header
            className={cn(
                'relative z-30 flex h-11 shrink-0 select-none items-center px-3',
                'bg-transparent',
                className,
            )}
        >
            <div className="h-full flex-1" data-tauri-drag-region />

            {terminalEnabled && !isToolWindow && (
                <div className="mr-2 flex items-center">
                    <TerminalToggleButton />
                </div>
            )}

            <div className="flex items-center gap-0.5">
                <button
                    type="button"
                    onClick={minimize}
                    title="最小化"
                    aria-label="最小化"
                    className={cn(
                        'flex h-7 w-7 items-center justify-center rounded-md text-text-tertiary',
                        'transition-all duration-150 ease-out hover:bg-warning/15 hover:text-warning active:scale-90',
                        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-warning',
                    )}
                >
                    <Minus size={13} strokeWidth={2.2} />
                </button>

                <button
                    type="button"
                    onClick={toggleMaximize}
                    title={isMaximized ? '向下还原' : '最大化'}
                    aria-label={isMaximized ? '还原' : '最大化'}
                    className={cn(
                        'flex h-7 w-7 items-center justify-center rounded-md text-text-tertiary',
                        'transition-all duration-150 ease-out hover:bg-success/15 hover:text-success active:scale-90',
                        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-success',
                    )}
                >
                    {isMaximized ? (
                        <Copy size={11} strokeWidth={2.2} />
                    ) : (
                        <Square size={11} strokeWidth={2.2} className="rounded-[2px]" />
                    )}
                </button>

                <button
                    type="button"
                    onClick={onClose}
                    title="关闭"
                    aria-label="关闭"
                    className={cn(
                        'flex h-7 w-7 items-center justify-center rounded-md text-text-tertiary',
                        'transition-all duration-150 ease-out hover:bg-danger hover:text-white active:scale-90',
                        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-danger',
                    )}
                >
                    <X size={13} strokeWidth={2.2} />
                </button>
            </div>
        </header>
    );
};

export default CustomTitleBar;
