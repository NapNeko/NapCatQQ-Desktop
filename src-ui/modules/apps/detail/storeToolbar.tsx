// Karin 插件市场和应用端商店共用的工具条（搜索 / 筛选 / 条数 / 刷新）和卸载确认。
// 工具条挂进详情页内容区右上的槽位：切走商店页时跟着卸掉，槽位空了整行就不占位。

import { useEffect, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { RefreshCw, Search } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Select,
    type SelectItem,
} from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { cn } from '../../../shared/utils/cn';

export const STORE_TOOLBAR_SLOT_ID = 'app-store-toolbar-slot';

function ToolbarPortal({ children }: { children: ReactNode }) {
    const [dock, setDock] = useState<HTMLElement | null>(() => document.getElementById(STORE_TOOLBAR_SLOT_ID));
    useEffect(() => {
        setDock(document.getElementById(STORE_TOOLBAR_SLOT_ID));
    }, []);
    if (!dock) return null;
    return createPortal(children, dock);
}

export function StoreToolbar<V extends string>({
    noun,
    query,
    onQueryChange,
    filters,
    filter,
    onFilterChange,
    count,
    loading,
    onReload,
}: {
    /** 「插件」「适配器」，读屏用 */
    noun: string;
    query: string;
    onQueryChange: (query: string) => void;
    filters: ReadonlyArray<SelectItem<V>>;
    filter: V;
    onFilterChange: (filter: V) => void;
    count: string;
    loading: boolean;
    onReload: () => void;
}) {
    return (
        <ToolbarPortal>
            <div className="flex min-w-0 items-center gap-1.5 pr-1">
                <div className="relative min-w-0">
                    <Search
                        size={13}
                        className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-text-tertiary"
                    />
                    <input
                        type="search"
                        aria-label={`搜索${noun}`}
                        placeholder="搜索"
                        value={query}
                        onChange={(e) => onQueryChange(e.target.value)}
                        className={cn(
                            'h-7 w-36 rounded-sm border border-transparent bg-inset/70 pl-7 pr-2',
                            'text-[12px] text-text outline-none transition-colors',
                            'placeholder:text-text-tertiary',
                            'hover:border-border-subtle hover:bg-inset',
                            'focus:border-brand focus:bg-field focus:ring-2 focus:ring-brand focus:ring-inset',
                            'sm:w-48',
                        )}
                    />
                </div>
                <Select
                    items={filters}
                    value={filter}
                    onValueChange={onFilterChange}
                    className="w-[6.75rem] shrink-0 [&_button]:h-7 [&_button]:min-h-7 [&_button]:px-2 [&_button]:py-0 [&_button]:text-[12px]"
                />
                <span className="hidden min-w-[1.25rem] text-right text-2xs tabular-nums text-text-tertiary sm:inline">
                    {count}
                </span>
                <Button variant="ghost" size="icon" className="h-7 w-7 shrink-0" aria-label="刷新" onClick={onReload}>
                    <ActionMotionIcon icon={RefreshCw} size={13} motion={loading ? 'spin' : 'none'} />
                </Button>
            </div>
        </ToolbarPortal>
    );
}

export const UninstallDialog: React.FC<{
    noun: string;
    /** 要卸的那个；null 时对话框关着 */
    target: string | null;
    onClose: () => void;
    onConfirm: (target: string) => void;
}> = ({ noun, target, onClose, onConfirm }) => (
    <Dialog open={target !== null} onOpenChange={(o) => !o && onClose()}>
        <DialogContent size="sm">
            <DialogHeader>
                <DialogTitle>卸载{noun}？</DialogTitle>
                <DialogDescription>此操作不可撤销。</DialogDescription>
            </DialogHeader>
            <DialogFooter>
                <Button variant="ghost" size="sm" onClick={onClose}>
                    取消
                </Button>
                <Button
                    variant="danger"
                    size="sm"
                    onClick={() => {
                        if (target) onConfirm(target);
                        onClose();
                    }}
                >
                    卸载
                </Button>
            </DialogFooter>
        </DialogContent>
    </Dialog>
);
