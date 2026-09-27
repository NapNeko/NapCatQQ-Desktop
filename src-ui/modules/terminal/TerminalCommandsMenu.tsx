// 常用命令 + 最近命令：点一下填进输入行，不直接执行，回车才跑。
// 框架预置的在上面（麦麦的 uv pip list 之类），自己加的在中间，最近跑过的在下面（按开终端的目标各记一份）。

import { useState } from 'react';
import { History, Plus, Sparkles, Trash2, X } from 'lucide-react';
import { Button, Popover, PopoverContent, PopoverTrigger, TextField } from '../../shared/ui';
import { terminalPrefs, useTerminalPrefs } from '../../hooks/terminal/terminalPrefs';
import { terminalRecents, useTerminalRecents } from '../../hooks/terminal/terminalRecents';
import type { TerminalSnippet } from '../../core/ipc/generated/domain/TerminalSnippet';

interface Props {
    targetKey: string;
    snippets: TerminalSnippet[];
    onPick(command: string): void;
}

function Row({ label, command, onPick, onRemove }: { label?: string; command: string; onPick(): void; onRemove?(): void }) {
    return (
        <div className="group flex items-center gap-1 rounded-xs pr-1 hover:bg-inset">
            <button type="button" onClick={onPick} className="min-w-0 flex-1 px-2 py-1 text-left" title={command}>
                {label && <span className="mr-2 text-[12px] text-text">{label}</span>}
                <span className="font-mono text-[11px] text-text-secondary">{command}</span>
            </button>
            {onRemove && (
                <button
                    type="button"
                    title="删掉"
                    onClick={onRemove}
                    className="hidden h-5 w-5 items-center justify-center rounded-xs text-text-tertiary hover:text-danger group-hover:flex"
                >
                    <X size={12} />
                </button>
            )}
        </div>
    );
}

function Section({ icon, title, action, children }: { icon: React.ReactNode; title: string; action?: React.ReactNode; children: React.ReactNode }) {
    return (
        <div className="py-1">
            <div className="flex items-center gap-1.5 px-2 pb-1 pt-1.5 text-[11px] text-text-tertiary">
                {icon}
                <span className="flex-1">{title}</span>
                {action}
            </div>
            {children}
        </div>
    );
}

export function TerminalCommandsMenu({ targetKey, snippets, onPick }: Props) {
    const [open, setOpen] = useState(false);
    const [adding, setAdding] = useState(false);
    const [label, setLabel] = useState('');
    const [command, setCommand] = useState('');
    const mine = useTerminalPrefs().snippets;
    const recents = useTerminalRecents(targetKey);

    const pick = (cmd: string) => {
        setOpen(false);
        onPick(cmd);
    };

    const add = () => {
        if (!command.trim()) return;
        terminalPrefs.patch({ snippets: [...mine, { label: label.trim(), command: command.trim() }] });
        setLabel('');
        setCommand('');
        setAdding(false);
    };

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    title="常用命令和最近命令"
                    className="flex h-6 items-center gap-1 rounded-xs px-1.5 text-[11px] text-text-tertiary transition-colors hover:bg-inset hover:text-text"
                >
                    <Sparkles size={12} />
                    命令
                </button>
            </PopoverTrigger>
            <PopoverContent align="end" className="max-h-[420px] w-[340px] overflow-y-auto p-1">
                {snippets.length > 0 && (
                    <Section icon={<Sparkles size={11} />} title="常用">
                        {snippets.map((s) => (
                            <Row key={`${s.label}-${s.command}`} label={s.label} command={s.command} onPick={() => pick(s.command)} />
                        ))}
                    </Section>
                )}

                <Section
                    icon={<Plus size={11} />}
                    title="我的命令"
                    action={
                        !adding && (
                            <button type="button" className="text-accent hover:underline" onClick={() => setAdding(true)}>
                                添加
                            </button>
                        )
                    }
                >
                    {mine.map((s, i) => (
                        <Row
                            key={`${i}-${s.command}`}
                            label={s.label || undefined}
                            command={s.command}
                            onPick={() => pick(s.command)}
                            onRemove={() => terminalPrefs.patch({ snippets: mine.filter((_, j) => j !== i) })}
                        />
                    ))}
                    {mine.length === 0 && !adding && (
                        <p className="px-2 py-1 text-[11px] text-text-tertiary">常敲的命令存在这里，所有终端都能用</p>
                    )}
                    {adding && (
                        <form
                            className="flex flex-col gap-1.5 px-2 py-1"
                            onSubmit={(e) => {
                                e.preventDefault();
                                add();
                            }}
                        >
                            <TextField
                                value={label}
                                onValueChange={setLabel}
                                placeholder="名字（可不填）"
                                aria-label="名字"
                                className="h-7 py-1 text-[12px]"
                            />
                            <TextField
                                autoFocus
                                value={command}
                                onValueChange={setCommand}
                                placeholder="命令，比如 docker ps"
                                aria-label="命令"
                                className="h-7 py-1 font-mono text-[12px]"
                            />
                            <div className="flex justify-end gap-1.5">
                                <Button size="sm" variant="ghost" onClick={() => setAdding(false)}>
                                    取消
                                </Button>
                                <Button size="sm" variant="primary" type="submit" disabled={!command.trim()}>
                                    存下
                                </Button>
                            </div>
                        </form>
                    )}
                </Section>

                <Section
                    icon={<History size={11} />}
                    title="最近"
                    action={
                        recents.length > 0 && (
                            <button
                                type="button"
                                title="清空这个终端的最近命令"
                                className="flex items-center gap-0.5 hover:text-danger"
                                onClick={() => terminalRecents.clear(targetKey)}
                            >
                                <Trash2 size={11} />
                                清空
                            </button>
                        )
                    }
                >
                    {recents.slice(0, 20).map((cmd) => (
                        <Row key={cmd} command={cmd} onPick={() => pick(cmd)} />
                    ))}
                    {recents.length === 0 && (
                        <p className="px-2 py-1 text-[11px] leading-relaxed text-text-tertiary">
                            还没有。带密码、密钥的命令和空格开头的命令不记
                        </p>
                    )}
                </Section>
            </PopoverContent>
        </Popover>
    );
}
