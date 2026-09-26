// 小节标题右边的「高级选项」：每节各自展开，展开了哪些整个详情页共用，切页回来还开着。

import { useCallback, useEffect, useState } from 'react';
import { ChevronDown } from 'lucide-react';
import { Button } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';

export interface AdvancedSections {
    isOpen: (key: string) => boolean;
    toggle: (key: string) => void;
    /** 展开这些小节，已经开着的不动 */
    reveal: (keys: readonly string[]) => void;
}

export function useAdvancedSections(): AdvancedSections {
    const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());
    const toggle = useCallback((key: string) => {
        setOpen((prev) => {
            const next = new Set(prev);
            if (!next.delete(key)) next.add(key);
            return next;
        });
    }, []);
    const reveal = useCallback((keys: readonly string[]) => {
        setOpen((prev) => (keys.every((k) => prev.has(k)) ? prev : new Set([...prev, ...keys])));
    }, []);
    return { isOpen: (key) => open.has(key), toggle, reveal };
}

/**
 * 收起的高级字段上有校验错误时把那一节展开，不然跳到这页也找不到错在哪。
 * 只在出错的小节有变化时展开一次，用户看过再收起不硬拦。
 */
export function useRevealOnError(advanced: AdvancedSections, keys: readonly string[]) {
    const joined = keys.join('\n');
    const { reveal } = advanced;
    useEffect(() => {
        if (joined) reveal(joined.split('\n'));
    }, [joined, reveal]);
}

export const AdvancedToggle: React.FC<{ open: boolean; onToggle: () => void }> = ({ open, onToggle }) => (
    <Button
        size="sm"
        variant="ghost"
        aria-expanded={open}
        onClick={onToggle}
        // 行里只有它时往右收，箭头和下面输入框的右边对齐
        className={cn('gap-1 px-2 last:-mr-2', open ? 'text-text-secondary' : 'text-text-tertiary')}
    >
        高级选项
        <ChevronDown
            size={13}
            aria-hidden
            className={cn('transition-transform duration-200', open && 'rotate-180')}
        />
    </Button>
);
