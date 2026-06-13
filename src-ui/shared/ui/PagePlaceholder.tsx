// 页面级占位：仅居中内容，无背景/边框卡片。

import React from 'react';
import { cn } from '../utils/cn';

export interface PagePlaceholderProps {
    children: React.ReactNode;
    className?: string;
}

export const PagePlaceholder: React.FC<PagePlaceholderProps> = ({ children, className }) => (
    <div
        className={cn(
            'flex flex-1 flex-col items-center justify-center gap-3 px-6 py-16 text-center',
            className,
        )}
    >
        {children}
    </div>
);