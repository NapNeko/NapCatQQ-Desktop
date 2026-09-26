// 两个 shared 里没有的列表控件。
// LineListField：一行一条。正则、句子、命令行参数里本身就有空格和逗号，StringListField 按分隔符拆会拆坏。
// StringMapEditor：字符串到字符串的映射（请求头、环境变量）。KeyValueListEditor 是给 .env 用的，
// 会把键转大写，请求头和查询参数大小写有意义，不能用它。

import { useEffect, useState, type ReactNode } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { Button, TextField } from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { cn } from '../../../../shared/utils/cn';

const RemoveButton: React.FC<{ onClick: () => void; disabled?: boolean }> = ({ onClick, disabled }) => (
    <button
        type="button"
        aria-label="删掉这一条"
        disabled={disabled}
        onClick={onClick}
        className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-xs text-text-tertiary hover:bg-danger-soft hover:text-danger disabled:opacity-40"
    >
        <Trash2 size={14} strokeWidth={2.2} />
    </button>
);

const FieldHead: React.FC<{ label?: ReactNode; hint?: ReactNode; error?: ReactNode; onAdd: () => void; disabled?: boolean }> = ({
    label,
    hint,
    error,
    onAdd,
    disabled,
}) => (
    <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
            {label && <span className="text-xs font-medium text-text-secondary">{label}</span>}
            {(error || hint) && (
                <p className={cn('text-2xs leading-snug', error ? 'text-danger' : 'text-text-tertiary')}>{error ?? hint}</p>
            )}
        </div>
        <Button size="sm" variant="ghost" disabled={disabled} onClick={onAdd}>
            <ActionMotionIcon icon={Plus} size={13} />
            加一条
        </Button>
    </div>
);

export const LineListField: React.FC<{
    label?: ReactNode;
    hint?: ReactNode;
    error?: ReactNode;
    /** 按下标给的错误，如 `regex/2` 那一行写错了 */
    itemErrors?: Record<number, string | undefined>;
    value: string[];
    onChange: (next: string[]) => void;
    placeholder?: string;
    mono?: boolean;
    disabled?: boolean;
    className?: string;
}> = ({ label, hint, error, itemErrors, value, onChange, placeholder, mono, disabled, className }) => (
    <div className={cn('flex flex-col gap-1.5', className)}>
        <FieldHead label={label} hint={hint} error={error} disabled={disabled} onAdd={() => onChange([...value, ''])} />
        {value.length === 0 ? (
            <p className="text-2xs text-text-tertiary">还没有</p>
        ) : (
            value.map((line, i) => (
                <div key={i} className="flex items-start gap-2">
                    <TextField
                        aria-label={`第 ${i + 1} 条`}
                        className={cn('min-w-0 flex-1', mono && 'font-mono')}
                        value={line}
                        placeholder={placeholder}
                        error={itemErrors?.[i]}
                        disabled={disabled}
                        onValueChange={(v) => onChange(value.map((s, j) => (j === i ? v : s)))}
                    />
                    <RemoveButton disabled={disabled} onClick={() => onChange(value.filter((_, j) => j !== i))} />
                </div>
            ))
        )}
    </div>
);

type Row = { key: string; value: string };

const toRows = (map: Record<string, string>): Row[] => Object.entries(map).map(([key, value]) => ({ key, value }));

const fromRows = (rows: Row[]): Record<string, string> =>
    Object.fromEntries(rows.filter((r) => r.key.trim()).map((r) => [r.key.trim(), r.value]));

const sameMap = (a: Record<string, string>, b: Record<string, string>) => {
    const ka = Object.keys(a);
    return ka.length === Object.keys(b).length && ka.every((k) => b[k] === a[k]);
};

export const StringMapEditor: React.FC<{
    label?: ReactNode;
    hint?: ReactNode;
    error?: ReactNode;
    value: Record<string, string>;
    onChange: (next: Record<string, string>) => void;
    keyPlaceholder?: string;
    valuePlaceholder?: string;
    disabled?: boolean;
    className?: string;
}> = ({ label, hint, error, value, onChange, keyPlaceholder = '名字', valuePlaceholder = '值', disabled, className }) => {
    // 行放在本地：刚加的空行、还没填键的行不进配置，但也不能因为值没变就被下一次渲染吃掉。
    // 外面的值变了（重置、重新读取）且和本地行对不上时才重铺
    const [rows, setRows] = useState(() => toRows(value));
    useEffect(() => {
        setRows((cur) => (sameMap(fromRows(cur), value) ? cur : toRows(value)));
    }, [value]);
    const change = (next: Row[]) => {
        setRows(next);
        onChange(fromRows(next));
    };
    return (
        <div className={cn('flex flex-col gap-1.5', className)}>
            <FieldHead
                label={label}
                hint={hint}
                error={error}
                disabled={disabled}
                onAdd={() => change([...rows, { key: '', value: '' }])}
            />
            {rows.length === 0 ? (
                <p className="text-2xs text-text-tertiary">还没有</p>
            ) : (
                rows.map((row, i) => (
                    <div key={i} className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)_auto] items-start gap-2">
                        <TextField
                            aria-label="名字"
                            className="font-mono"
                            value={row.key}
                            placeholder={keyPlaceholder}
                            disabled={disabled}
                            onValueChange={(v) => change(rows.map((r, j) => (j === i ? { ...r, key: v } : r)))}
                        />
                        <TextField
                            aria-label="值"
                            className="font-mono"
                            value={row.value}
                            placeholder={valuePlaceholder}
                            disabled={disabled}
                            onValueChange={(v) => change(rows.map((r, j) => (j === i ? { ...r, value: v } : r)))}
                        />
                        <RemoveButton disabled={disabled} onClick={() => change(rows.filter((_, j) => j !== i))} />
                    </div>
                ))
            )}
        </div>
    );
};
