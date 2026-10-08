// 数组 / 字典共用的删除按钮。

import { Trash2 } from 'lucide-react';

export function ItemRemove({ onClick, disabled }: { onClick: () => void; disabled?: boolean }) {
    return (
        <button
            type="button"
            aria-label="删除这一项"
            title="删除这一项"
            disabled={disabled}
            onClick={onClick}
            className="mt-6 inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-text-tertiary transition-colors hover:bg-danger-soft hover:text-danger disabled:opacity-40"
        >
            <Trash2 size={14} />
        </button>
    );
}
