// className 合并工具：tailwind-merge + clsx 经典组合。shadcn 标配。
// 用法: cn('px-2', isActive && 'bg-brand', className)

import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]): string {
    return twMerge(clsx(inputs));
}
