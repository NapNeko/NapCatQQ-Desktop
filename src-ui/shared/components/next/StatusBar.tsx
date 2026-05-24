// 底部状态栏（next）。h=24，背景透明，靠极弱字色 + dot 出存在感。
// 不接 hook，纯 props。

import React from 'react';
import { cn } from '../../utils/cn';

export interface StatusBarProps {
    dataRoot?: string;
    appVersion?: string;
    /** 'connected' 显示绿点，'connecting' 显示橙点，'disconnected' 灰点。 */
    connectionState?: 'connected' | 'connecting' | 'disconnected';
    className?: string;
}

const dotClass: Record<NonNullable<StatusBarProps['connectionState']>, string> = {
    connected: 'bg-success shadow-glow-success',
    connecting: 'bg-warning',
    disconnected: 'bg-text-disabled',
};

const dotLabel: Record<NonNullable<StatusBarProps['connectionState']>, string> = {
    connected: 'Tauri 通道已就绪',
    connecting: '正在连接 Tauri 后端',
    disconnected: 'Tauri 通道未就绪',
};

export const StatusBar: React.FC<StatusBarProps> = ({
    dataRoot = '%ProgramData%\\NapCatQQ Desktop',
    appVersion = 'v0.1.0-alpha.1',
    connectionState = 'disconnected',
    className,
}) => {
    return (
        <footer
            className={cn(
                'relative z-10 flex h-6 shrink-0 select-none items-center justify-between gap-3 px-3',
                'text-[11px] text-text-disabled',
                className,
            )}
        >
            <div className="flex min-w-0 items-center gap-2">
                <span
                    aria-label={dotLabel[connectionState]}
                    title={dotLabel[connectionState]}
                    className={cn('inline-block h-1.5 w-1.5 shrink-0 rounded-full', dotClass[connectionState])}
                />
                <span className="truncate font-mono">{dataRoot}</span>
            </div>

            <span className="shrink-0 font-mono">{appVersion}</span>
        </footer>
    );
};

export default StatusBar;
