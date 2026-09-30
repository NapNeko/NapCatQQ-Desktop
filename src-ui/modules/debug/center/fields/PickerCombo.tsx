// 「输入框 + 候选列表」的组合框：群号、好友、群成员、消息 id 四种选择器共用。
//
// 输入框里永远可以直接填（候选拉不到、Bot 没在跑都不挡填写）；敲字同时按名字或号码筛候选。
// 焦点始终留在输入框里：列表只用鼠标点或 ↑↓ + 回车挑，Esc 先收起列表（这时不会误触「取消调用」）。

import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { ChevronDown, RefreshCw } from 'lucide-react';
import { coerceInput } from '../../../../core/domain/debug/schemaForm';
import { cn } from '../../../../shared/utils/cn';
import { Popover, PopoverAnchor, PopoverContent, Spinner } from '../../../../shared/ui';
import { FIELD_INPUT_CLASS, IconTip, fieldBorder } from '../centerParts';
import { valueText } from '../viewHelpers';
import { useTextDraft, type FieldProps } from './fieldKit';

export interface PickerOption {
    id: number | string;
    label: string;
    hint?: string;
}

/** 列表最多画这么多行：几千人的群，敲两个字就筛下来了，没必要一次画完 */
const MAX_SHOWN = 100;

export interface PickerComboProps extends FieldProps {
    options: readonly PickerOption[];
    loading?: boolean;
    /** 拉取候选失败的原话；有它时只剩输入框 */
    error?: string | null;
    onRefresh?: () => void;
    /** 候选眼下拿不到的原因（Bot 没在跑、还没填群号……）；有它时列表收起、按钮禁用 */
    unavailable?: string | null;
    emptyText: string;
    placeholder?: string;
    /** 选择器是哪一类，给屏幕阅读器和提示用 */
    noun: string;
}

export function PickerCombo({
    field,
    value,
    onChange,
    invalid,
    inputId,
    describedBy,
    disabled,
    options,
    loading = false,
    error = null,
    onRefresh,
    unavailable = null,
    emptyText,
    placeholder,
    noun,
}: PickerComboProps) {
    const [text, setText] = useTextDraft(value, valueText, (t) => coerceInput(field, t), onChange);
    const [open, setOpen] = useState(false);
    // 刚打开列表时不按已填的号筛（否则只剩一行）；用户一敲字就开始筛
    const [typed, setTyped] = useState(false);
    // 用户在列表里动过（敲字、方向键、鼠标划过）之后回车才算「挑这一项」，刚点开就回车只是收起
    const [armed, setArmed] = useState(false);
    const [highlight, setHighlight] = useState(0);
    const anchorRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);
    const listId = useId();
    const canList = !unavailable && !error && !disabled;

    const q = typed ? text.trim().toLowerCase() : '';
    const matches = useMemo(() => {
        if (!q) return options;
        return options.filter(
            (o) =>
                String(o.id).includes(q) ||
                o.label.toLowerCase().includes(q) ||
                (o.hint ? o.hint.toLowerCase().includes(q) : false),
        );
    }, [options, q]);
    const shown = matches.length > MAX_SHOWN ? matches.slice(0, MAX_SHOWN) : matches;
    const selected = useMemo(() => options.find((o) => String(o.id) === text.trim()) ?? null, [options, text]);
    const hl = Math.min(highlight, Math.max(0, shown.length - 1));
    const listOpen = open && canList;

    useEffect(() => {
        if (!listOpen) return;
        listRef.current?.querySelector<HTMLElement>(`[data-index="${hl}"]`)?.scrollIntoView({ block: 'nearest' });
    }, [hl, listOpen]);

    const openList = () => {
        if (!canList) return;
        setTyped(false);
        setArmed(false);
        setHighlight(Math.max(0, options.findIndex((o) => String(o.id) === text.trim())));
        setOpen(true);
    };

    const choose = (o: PickerOption) => {
        setText(String(o.id));
        setOpen(false);
        setTyped(false);
        inputRef.current?.focus();
    };

    const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.nativeEvent.isComposing) return;
        switch (e.key) {
            case 'ArrowDown':
                e.preventDefault();
                if (!listOpen) openList();
                else {
                    setArmed(true);
                    setHighlight(Math.min(shown.length - 1, hl + 1));
                }
                break;
            case 'ArrowUp':
                if (!listOpen) return;
                e.preventDefault();
                setArmed(true);
                setHighlight(Math.max(0, hl - 1));
                break;
            case 'Enter': {
                if (!listOpen || e.ctrlKey || e.metaKey) return;
                e.preventDefault();
                const o = shown[hl];
                if (o && armed) choose(o);
                else setOpen(false);
                break;
            }
            case 'Escape':
                if (!listOpen) return;
                // 拦下来：页面上的 Esc 是「取消调用」，这一下只该收起列表
                e.preventDefault();
                setOpen(false);
                break;
            case 'Tab':
                setOpen(false);
                break;
            default:
                break;
        }
    };

    let body: ReactNode;
    if (loading && options.length === 0) {
        body = (
            <div className="flex items-center justify-center gap-2 px-3 py-5 text-xs text-text-tertiary">
                <Spinner size="xs" />
                正在拉取{noun}列表…
            </div>
        );
    } else if (options.length === 0) {
        body = <p className="px-3 py-4 text-center text-xs text-text-tertiary">{emptyText}</p>;
    } else if (shown.length === 0) {
        body = (
            <p className="px-3 py-4 text-center text-xs text-text-tertiary">
                没有匹配的{noun}，填的「{text.trim()}」照样可以发
            </p>
        );
    } else {
        body = shown.map((o, i) => (
            <div
                key={`${o.id}`}
                id={`${listId}-${i}`}
                role="option"
                aria-selected={i === hl}
                data-index={i}
                onMouseDown={(e) => e.preventDefault()}
                onMouseMove={() => {
                    if (i === hl) return;
                    setArmed(true);
                    setHighlight(i);
                }}
                onClick={() => choose(o)}
                className={cn(
                    'flex cursor-pointer items-baseline gap-2 rounded-xs px-2 py-1.5',
                    i === hl ? 'bg-inset' : '',
                    selected?.id === o.id && 'text-brand',
                )}
            >
                <span className="min-w-0 flex-1 truncate text-[13px] text-text">{o.label}</span>
                <span className="shrink-0 truncate font-mono text-[11px] text-text-tertiary">{o.hint ?? String(o.id)}</span>
            </div>
        ));
    }

    const hint = error ? `拉取列表失败：${error}，可以直接填号` : unavailable;

    return (
        <div className="flex flex-col gap-1">
            <Popover open={listOpen} onOpenChange={(next) => !next && setOpen(false)}>
                <PopoverAnchor asChild>
                    <div ref={anchorRef} className="relative flex items-center">
                        <input
                            ref={inputRef}
                            id={inputId}
                            type="text"
                            role="combobox"
                            aria-expanded={listOpen}
                            aria-controls={listOpen ? listId : undefined}
                            aria-activedescendant={listOpen && shown[hl] ? `${listId}-${hl}` : undefined}
                            aria-autocomplete="list"
                            aria-invalid={invalid || undefined}
                            aria-describedby={describedBy}
                            value={text}
                            disabled={disabled}
                            placeholder={placeholder ?? (canList ? `填号，或敲名字从${noun}里挑` : '直接填号')}
                            onChange={(e) => {
                                setText(e.target.value);
                                setTyped(true);
                                setArmed(true);
                                setHighlight(0);
                                if (canList) setOpen(true);
                            }}
                            onClick={() => (listOpen ? undefined : openList())}
                            onKeyDown={onKeyDown}
                            onBlur={() => setOpen(false)}
                            spellCheck={false}
                            autoComplete="off"
                            className={cn(FIELD_INPUT_CLASS, fieldBorder(invalid), 'pr-[4.5rem] font-mono text-[13px]')}
                        />
                        <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center gap-0.5 pr-1">
                            {selected && (
                                <span className="max-w-[9rem] truncate pr-1 text-xs text-text-tertiary">{selected.label}</span>
                            )}
                            <span className="pointer-events-auto flex items-center gap-0.5">
                                {loading ? (
                                    <span className="inline-flex h-6 w-6 items-center justify-center">
                                        <Spinner size="xs" label={`正在拉取${noun}列表`} />
                                    </span>
                                ) : (
                                    onRefresh && (
                                        <IconTip
                                            icon={RefreshCw}
                                            label={`刷新${noun}列表`}
                                            size="sm"
                                            tabIndex={-1}
                                            disabled={!!unavailable || disabled}
                                            onMouseDown={(e) => e.preventDefault()}
                                            onClick={onRefresh}
                                        />
                                    )
                                )}
                                <IconTip
                                    icon={ChevronDown}
                                    label={listOpen ? `收起${noun}列表` : `展开${noun}列表`}
                                    size="sm"
                                    tabIndex={-1}
                                    disabled={!canList}
                                    onMouseDown={(e) => e.preventDefault()}
                                    onClick={() => {
                                        if (listOpen) setOpen(false);
                                        else openList();
                                        inputRef.current?.focus();
                                    }}
                                />
                            </span>
                        </div>
                    </div>
                </PopoverAnchor>
                <PopoverContent
                    align="start"
                    sideOffset={4}
                    className="w-[var(--radix-popover-trigger-width)] min-w-[260px] max-w-[440px] p-1"
                    onOpenAutoFocus={(e) => e.preventDefault()}
                    onCloseAutoFocus={(e) => e.preventDefault()}
                    onInteractOutside={(e) => {
                        if (anchorRef.current?.contains(e.target as Node)) e.preventDefault();
                    }}
                >
                    <div
                        ref={listRef}
                        id={listId}
                        role="listbox"
                        aria-label={`${noun}列表`}
                        // 点列表（含滚动条）不许把焦点从输入框抢走，否则输入框失焦会把列表收起
                        onMouseDown={(e) => e.preventDefault()}
                        className="max-h-64 overflow-y-auto"
                    >
                        {body}
                        {matches.length > MAX_SHOWN && (
                            <p className="px-2 py-1.5 text-center text-2xs text-text-tertiary">
                                还有 {matches.length - MAX_SHOWN} 个没列出来，多敲几个字缩小范围
                            </p>
                        )}
                    </div>
                </PopoverContent>
            </Popover>
            {hint && <p className="text-2xs leading-snug text-text-tertiary">{hint}</p>}
        </div>
    );
}
