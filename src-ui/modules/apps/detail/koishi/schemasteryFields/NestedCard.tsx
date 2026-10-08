// 嵌套卡片：数组 / 字典 / 对象字段的容器，标题行可折叠。

import { ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { Button } from '../../../../../shared/ui';
import { cn } from '../../../../../shared/utils/cn';
import type { SNode } from '../../../../../core/domain/apps/koishiSchema';
import { FieldLabel, fieldText } from '../schemaLabel';
import { WIDE } from '../schemasteryModel';

/** 嵌套对象 / 列表 / 字典：带边框的卡片，标题行可折叠；schema 标了 collapse 的默认收起 */
export function NestedCard({
    name,
    node,
    onReset,
    disabled,
    actions,
    count,
    children,
}: {
    name: string;
    node: SNode;
    onReset?: () => void;
    disabled?: boolean;
    actions?: React.ReactNode;
    count?: number;
    children: React.ReactNode;
}) {
    const [open, setOpen] = useState(!node.meta.collapse);
    const { hint } = fieldText(name, node);
    return (
        <div
            className={cn(
                'flex min-w-0 flex-col rounded-lg border border-border-subtle bg-surface/60',
                WIDE,
            )}
        >
            <div className="flex min-h-[44px] items-center gap-2 px-3.5 py-2">
                <button
                    type="button"
                    aria-expanded={open}
                    className="flex min-w-0 flex-1 items-center gap-2 text-left text-[13px] font-medium text-text"
                    onClick={() => setOpen((v) => !v)}
                >
                    <ChevronRight
                        size={14}
                        className={cn(
                            'shrink-0 text-text-tertiary transition-transform duration-200',
                            open && 'rotate-90',
                        )}
                    />
                    <FieldLabel name={name} node={node} />
                    {count !== undefined && (
                        <span className="rounded-pill bg-inset px-1.5 text-2xs font-normal tabular-nums text-text-tertiary">
                            {count}
                        </span>
                    )}
                </button>
                <div className="flex shrink-0 items-center gap-1">
                    {actions}
                    {onReset && (
                        <Button size="sm" variant="ghost" disabled={disabled} onClick={onReset}>
                            恢复默认
                        </Button>
                    )}
                </div>
            </div>
            {hint && (
                <p className="-mt-1 px-3.5 pb-2 pl-9 text-2xs leading-relaxed text-text-tertiary">
                    {hint}
                </p>
            )}
            {open && <div className="border-t border-border-subtle/70 px-3.5 py-3">{children}</div>}
        </div>
    );
}
