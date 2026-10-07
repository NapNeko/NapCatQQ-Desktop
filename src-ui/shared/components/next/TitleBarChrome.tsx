// 主窗和工具窗共用窗口控制；终端等主窗能力由调用方挂载。
import { Copy, Minus, Square, X } from 'lucide-react';
import type { ReactNode } from 'react';
import { useWindowControls } from '../../../hooks/desktop/useWindowControls';
import { cn } from '../../utils/cn';

export function TitleBarChrome({
    className,
    tool = false,
    children,
}: {
    className?: string;
    tool?: boolean;
    children?: ReactNode;
}) {
    const { isMaximized, minimize, toggleMaximize, close, closeSelf } = useWindowControls();
    return (
        <header
            className={cn(
                'relative z-30 flex h-11 shrink-0 select-none items-center px-3',
                'bg-transparent',
                className,
            )}
        >
            <div className="h-full flex-1" data-tauri-drag-region />
            {children && <div className="mr-2 shrink-0">{children}</div>}
            <div className="flex items-center gap-0.5">
                <button
                    type="button"
                    onClick={minimize}
                    title="最小化"
                    aria-label="最小化"
                    className={cn(
                        'flex h-7 w-7 items-center justify-center rounded-md text-text-tertiary',
                        'transition-all duration-150 ease-out hover:bg-warning/15 hover:text-warning active:scale-95',
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
                        'transition-all duration-150 ease-out hover:bg-success/15 hover:text-success active:scale-95',
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
                    onClick={tool ? closeSelf : close}
                    title="关闭"
                    aria-label="关闭"
                    className={cn(
                        'flex h-7 w-7 items-center justify-center rounded-md text-text-tertiary',
                        'transition-all duration-150 ease-out hover:bg-danger hover:text-white active:scale-95',
                        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-danger',
                    )}
                >
                    <X size={13} strokeWidth={2.2} />
                </button>
            </div>
        </header>
    );
}
