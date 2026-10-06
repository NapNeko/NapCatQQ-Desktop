// 常用命令 + 最近命令：点一下填进输入行，不直接执行，回车才跑。
// 框架预置的在上面（麦麦的 uv pip list 之类），自己加的在中间，最近跑过的在下面（按开终端的目标各记一份）。
// 顶上的筛选框三组一起筛；命令多了靠它找，不用翻。

import { useMemo, useRef, useState } from 'react';
import { Bookmark, CornerDownLeft, History, Plus, Search, Sparkles, Trash2, X } from 'lucide-react';
import { Button, Popover, PopoverContent, PopoverTrigger } from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { terminalPrefs, useTerminalPrefs } from '../../hooks/terminal/terminalPrefs';
import { terminalRecents, useTerminalRecents } from '../../hooks/terminal/terminalRecents';
import type { TerminalSnippet } from '../../core/ipc/generated/domain/TerminalSnippet';

interface Props {
    targetKey: string;
    snippets: TerminalSnippet[];
    onPick(command: string): void;
}

const FIELD =
    'h-7 w-full rounded-xs border border-border-subtle bg-field px-2 text-[12px] text-text outline-none ' +
    'transition-[border-color,box-shadow] placeholder:text-text-tertiary focus:border-accent focus:ring-2 focus:ring-accent/20';

function matches(query: string, ...texts: (string | undefined)[]): boolean {
    if (!query) return true;
    const q = query.toLowerCase();
    return texts.some((t) => t?.toLowerCase().includes(q));
}

function Row({
    label,
    command,
    onPick,
    onRemove,
}: {
    label?: string;
    command: string;
    onPick(): void;
    onRemove?(): void;
}) {
    return (
        <div className="group relative flex items-center rounded-xs hover:bg-inset focus-within:bg-inset">
            <button
                type="button"
                onClick={onPick}
                title={`填进输入行：${command}`}
                className="flex min-w-0 flex-1 flex-col gap-px px-2 py-1.5 text-left outline-none"
            >
                {label && (
                    <span className="truncate text-[12px] leading-tight text-text">{label}</span>
                )}
                <span
                    className={cn(
                        'truncate font-mono leading-tight',
                        label
                            ? 'text-[11px] text-text-tertiary'
                            : 'text-[12px] text-text-secondary',
                    )}
                >
                    {command}
                </span>
            </button>
            <CornerDownLeft
                size={12}
                aria-hidden
                className="mr-1.5 shrink-0 text-text-tertiary opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
            />
            {onRemove && (
                <button
                    type="button"
                    title="删掉这条"
                    aria-label="删掉这条"
                    onClick={onRemove}
                    className="mr-1 flex h-5 w-5 shrink-0 items-center justify-center rounded-xs text-text-tertiary opacity-0 transition-opacity hover:bg-surface hover:text-danger focus-visible:opacity-100 group-hover:opacity-100"
                >
                    <X size={12} />
                </button>
            )}
        </div>
    );
}

function Section({
    icon,
    title,
    count,
    action,
    children,
}: {
    icon: React.ReactNode;
    title: string;
    count?: number;
    action?: React.ReactNode;
    children: React.ReactNode;
}) {
    return (
        <section className="py-1">
            <div className="flex h-6 items-center gap-1.5 px-2 text-[11px] font-medium text-text-tertiary">
                {icon}
                <span>{title}</span>
                {count ? (
                    <span className="tabular-nums font-normal opacity-70">{count}</span>
                ) : null}
                <span className="flex-1" />
                {action}
            </div>
            {children}
        </section>
    );
}

function Hint({ children }: { children: React.ReactNode }) {
    return <p className="px-2 py-1 text-[11px] leading-relaxed text-text-tertiary">{children}</p>;
}

function AddForm({ onDone }: { onDone(): void }) {
    const [label, setLabel] = useState('');
    const [command, setCommand] = useState('');
    const mine = useTerminalPrefs().snippets;
    const save = () => {
        if (!command.trim()) return;
        terminalPrefs.patch({
            snippets: [...mine, { label: label.trim(), command: command.trim() }],
        });
        onDone();
    };
    return (
        <form
            className="mx-1 mb-1 flex flex-col gap-1.5 rounded-sm border border-border-subtle bg-inset/60 p-2"
            onSubmit={(e) => {
                e.preventDefault();
                save();
            }}
            // Esc 只收起表单，不关整个菜单；回车自己接，输入法选字时的回车不算
            onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
                    e.preventDefault();
                    save();
                } else if (e.key === 'Escape') {
                    e.preventDefault();
                    e.stopPropagation();
                    onDone();
                }
            }}
        >
            <input
                autoFocus
                value={command}
                onChange={(e) => setCommand(e.target.value)}
                placeholder="命令，比如 docker ps"
                aria-label="命令"
                spellCheck={false}
                className={cn(FIELD, 'font-mono')}
            />
            <input
                value={label}
                onChange={(e) => setLabel(e.target.value)}
                placeholder="名字（可不填）"
                aria-label="名字"
                className={FIELD}
            />
            <div className="flex items-center gap-1.5 pt-0.5">
                <span className="flex-1 text-[10px] text-text-tertiary">回车存下 · Esc 取消</span>
                <Button size="sm" variant="ghost" className="h-6 px-2" onClick={onDone}>
                    取消
                </Button>
                <Button
                    size="sm"
                    variant="primary"
                    type="submit"
                    className="h-6 px-2.5"
                    disabled={!command.trim()}
                >
                    存下
                </Button>
            </div>
        </form>
    );
}

export function TerminalCommandsMenu({ targetKey, snippets, onPick }: Props) {
    const [open, setOpen] = useState(false);
    const [adding, setAdding] = useState(false);
    const [query, setQuery] = useState('');
    const searchRef = useRef<HTMLInputElement>(null);
    const mine = useTerminalPrefs().snippets;
    const recents = useTerminalRecents(targetKey);

    const q = query.trim();
    const shownPresets = useMemo(
        () => snippets.filter((s) => matches(q, s.label, s.command)),
        [snippets, q],
    );
    const shownMine = useMemo(
        () => mine.map((s, i) => ({ s, i })).filter(({ s }) => matches(q, s.label, s.command)),
        [mine, q],
    );
    const shownRecents = useMemo(
        () => recents.filter((c) => matches(q, c)).slice(0, 20),
        [recents, q],
    );
    const nothing =
        q && shownPresets.length === 0 && shownMine.length === 0 && shownRecents.length === 0;

    const pick = (cmd: string) => {
        setOpen(false);
        onPick(cmd);
    };

    return (
        <Popover
            open={open}
            onOpenChange={(next) => {
                setOpen(next);
                if (!next) {
                    setAdding(false);
                    setQuery('');
                }
            }}
        >
            <PopoverTrigger asChild>
                <button
                    type="button"
                    title="常用命令和最近命令"
                    aria-pressed={open}
                    className={cn(
                        'flex h-6 items-center gap-1 rounded-xs px-1.5 text-[11px] transition-colors',
                        open
                            ? 'bg-inset text-text'
                            : 'text-text-tertiary hover:bg-inset hover:text-text',
                    )}
                >
                    <Sparkles size={12} />
                    命令
                </button>
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="flex w-[360px] flex-col overflow-hidden p-0"
                onOpenAutoFocus={(e) => {
                    e.preventDefault();
                    searchRef.current?.focus();
                }}
            >
                <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border-subtle px-2.5">
                    <Search size={13} className="shrink-0 text-text-tertiary" />
                    <input
                        ref={searchRef}
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        onKeyDown={(e) => {
                            // 筛出来只剩一条时回车直接填
                            const only = [
                                ...shownPresets.map((s) => s.command),
                                ...shownMine.map((m) => m.s.command),
                                ...shownRecents,
                            ];
                            if (e.key === 'Enter' && q && only.length > 0) {
                                e.preventDefault();
                                pick(only[0] as string);
                            }
                        }}
                        placeholder="筛选命令"
                        aria-label="筛选命令"
                        spellCheck={false}
                        className="h-full min-w-0 flex-1 bg-transparent text-[12px] text-text outline-none placeholder:text-text-tertiary"
                    />
                    {query && (
                        <button
                            type="button"
                            aria-label="清掉筛选"
                            onClick={() => {
                                setQuery('');
                                searchRef.current?.focus();
                            }}
                            className="flex h-5 w-5 items-center justify-center rounded-xs text-text-tertiary hover:bg-inset hover:text-text"
                        >
                            <X size={12} />
                        </button>
                    )}
                </div>

                <div className="max-h-[360px] overflow-y-auto overscroll-contain p-1">
                    {nothing && <Hint>没有匹配「{q}」的命令</Hint>}

                    {shownPresets.length > 0 && (
                        <Section
                            icon={<Sparkles size={11} />}
                            title="常用"
                            count={shownPresets.length}
                        >
                            {shownPresets.map((s) => (
                                <Row
                                    key={`${s.label}-${s.command}`}
                                    label={s.label}
                                    command={s.command}
                                    onPick={() => pick(s.command)}
                                />
                            ))}
                        </Section>
                    )}

                    {(!q || shownMine.length > 0) && (
                        <Section
                            icon={<Bookmark size={11} />}
                            title="我的命令"
                            count={mine.length}
                            action={
                                !adding && (
                                    <button
                                        type="button"
                                        onClick={() => setAdding(true)}
                                        className="flex h-5 items-center gap-0.5 rounded-xs px-1 font-normal text-accent transition-colors hover:bg-accent-soft"
                                    >
                                        <Plus size={11} />
                                        添加
                                    </button>
                                )
                            }
                        >
                            {adding && <AddForm onDone={() => setAdding(false)} />}
                            {shownMine.map(({ s, i }) => (
                                <Row
                                    key={`${i}-${s.command}`}
                                    label={s.label || undefined}
                                    command={s.command}
                                    onPick={() => pick(s.command)}
                                    onRemove={() =>
                                        terminalPrefs.patch({
                                            snippets: mine.filter((_, j) => j !== i),
                                        })
                                    }
                                />
                            ))}
                            {mine.length === 0 && !adding && (
                                <Hint>常敲的命令存在这里，所有终端都能用</Hint>
                            )}
                        </Section>
                    )}

                    {(!q || shownRecents.length > 0) && (
                        <Section
                            icon={<History size={11} />}
                            title="最近"
                            action={
                                recents.length > 0 && (
                                    <button
                                        type="button"
                                        title="清空这个终端的最近命令"
                                        className="flex h-5 items-center gap-0.5 rounded-xs px-1 font-normal transition-colors hover:bg-danger-soft hover:text-danger"
                                        onClick={() => terminalRecents.clear(targetKey)}
                                    >
                                        <Trash2 size={11} />
                                        清空
                                    </button>
                                )
                            }
                        >
                            {shownRecents.map((cmd) => (
                                <Row key={cmd} command={cmd} onPick={() => pick(cmd)} />
                            ))}
                            {recents.length === 0 && (
                                <Hint>还没有。带密码、密钥的命令和空格开头的命令不记</Hint>
                            )}
                        </Section>
                    )}
                </div>

                <p className="flex h-7 shrink-0 items-center gap-1 border-t border-border-subtle px-3 text-[10px] text-text-tertiary">
                    <CornerDownLeft size={10} />
                    点一下填进输入行，回车才执行
                </p>
            </PopoverContent>
        </Popover>
    );
}
