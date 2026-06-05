// 底部状态栏（next）。仅展示应用版本，右对齐。

import React from 'react';
import { APP_VERSION_LABEL } from '../../../core/domain/app-meta';
import { cn } from '../../utils/cn';

export interface StatusBarProps {
    appVersion?: string;
    className?: string;
}

export const StatusBar: React.FC<StatusBarProps> = ({
    appVersion = APP_VERSION_LABEL,
    className,
}) => {
    return (
        <footer
            className={cn(
                'relative z-10 flex h-6 shrink-0 select-none items-center justify-end px-3',
                'text-[11px] text-text-disabled',
                className,
            )}
        >
            <span className="shrink-0 font-mono tabular-nums" title="NapCatQQ Desktop 应用版本">
                {appVersion}
            </span>
        </footer>
    );
};

export default StatusBar;