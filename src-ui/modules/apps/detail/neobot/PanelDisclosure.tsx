import { useId, useState, type ReactNode } from 'react';
import { ExpandChevron, ExpandPresence } from '../../../../shared/ui/motion';
import { cn } from '../../../../shared/utils/cn';

/** 保留编辑器草稿，同时让详情区的展开与收起遵循全局动效设置。 */
export function PanelDisclosure({
    label,
    children,
    defaultOpen = false,
    className,
    triggerClassName,
    description,
}: {
    label: ReactNode;
    children: ReactNode;
    defaultOpen?: boolean;
    className?: string;
    triggerClassName?: string;
    description?: string;
}) {
    const [open, setOpen] = useState(defaultOpen);
    const panelId = useId();
    return (
        <div className={cn('min-w-0', className)}>
            <button
                type="button"
                aria-expanded={open}
                aria-controls={panelId}
                title={description}
                className={cn(
                    'flex min-w-0 items-center gap-1.5 text-left text-xs text-text-secondary',
                    triggerClassName,
                )}
                onClick={() => setOpen((value) => !value)}
            >
                <ExpandChevron open={open} size={12} />
                <span className="min-w-0 break-words">{label}</span>
            </button>
            <ExpandPresence visible={open} keepMounted>
                <div id={panelId} className="pt-3">
                    {children}
                </div>
            </ExpandPresence>
        </div>
    );
}
