// 表单字段的标题：Koishi 插件只给了键名和一句描述，直接拿键名当标题太像在看源码。
// 描述的第一句够短就当标题，剩下的当说明；键名缩成标题旁的小字，照着文档找也对得上。

import { RotateCcw } from 'lucide-react';
import { describe, type SNode } from '../../../../core/domain/apps/koishiSchema';

const ROLE_UNIT: Record<string, string> = { ms: '毫秒', time: '毫秒' };
const TITLE_MAX = 26;

/** 描述拆成「标题 + 说明」；第一句太长就整段当说明、标题用键名 */
export function splitDescription(text: string): { title: string; rest: string } {
    const d = text.trim();
    if (!d) return { title: '', rest: '' };
    const m = d.match(/^(.+?)[。；;！!？?](.*)$/s);
    const head = (m ? m[1] : d).replace(/[，,：:\s]+$/, '');
    const tail = m ? m[2].trim() : '';
    if (head.length > TITLE_MAX || /`/.test(head)) return { title: '', rest: d };
    return { title: head, rest: tail };
}

export function fieldText(
    name: string,
    node: SNode,
): { title: string; hint?: string; showKey: boolean } {
    const { title, rest } = splitDescription(describe(node));
    const unit = node.meta.role ? ROLE_UNIT[node.meta.role] : undefined;
    const hintParts = [
        rest,
        unit && !`${title}${rest}`.includes('毫秒') ? `单位：${unit}` : '',
    ].filter(Boolean);
    return {
        title: title || name,
        hint: hintParts.length ? hintParts.join(' ') : undefined,
        showKey: !!title && !!name,
    };
}

/** 字段标题行：标题、键名小字、必填星号，右边可选「恢复默认」 */
export function FieldLabel({
    name,
    node,
    onReset,
    disabled,
}: {
    name: string;
    node: SNode;
    onReset?: () => void;
    disabled?: boolean;
}) {
    const { title, showKey } = fieldText(name, node);
    return (
        <span className="flex min-w-0 items-center gap-1.5">
            <span className="truncate">{title}</span>
            {node.meta.required && <span className="text-danger">*</span>}
            {showKey && (
                <span className="truncate font-mono text-2xs font-normal text-text-disabled">
                    {name}
                </span>
            )}
            {onReset && (
                <button
                    type="button"
                    title="恢复默认值"
                    aria-label="恢复默认值"
                    disabled={disabled}
                    className="ml-auto inline-flex h-5 w-5 shrink-0 items-center justify-center rounded text-text-tertiary transition-colors hover:bg-inset hover:text-brand disabled:opacity-40"
                    onClick={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        onReset();
                    }}
                >
                    <RotateCcw size={11} />
                </button>
            )}
        </span>
    );
}
