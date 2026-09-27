// Ctrl+Shift+F 的搜索条：浮在终端右上角，回车下一个、Shift+回车上一个、Esc 关掉。

import { useEffect, useRef, useState } from 'react';
import { ArrowDown, ArrowUp, CaseSensitive, Regex, WholeWord, X } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import type { TerminalRuntime } from './runtime';

interface Props {
    runtime: TerminalRuntime;
    onClose(): void;
}

function Toggle({ active, onClick, title, children }: { active: boolean; onClick(): void; title: string; children: React.ReactNode }) {
    return (
        <button
            type="button"
            title={title}
            aria-pressed={active}
            onClick={onClick}
            className={cn(
                'flex h-6 w-6 items-center justify-center rounded-xs transition-colors',
                active ? 'bg-accent-soft text-text' : 'text-text-tertiary hover:bg-inset hover:text-text',
            )}
        >
            {children}
        </button>
    );
}

export function TerminalSearchBar({ runtime, onClose }: Props) {
    const inputRef = useRef<HTMLInputElement>(null);
    const [query, setQuery] = useState('');
    const [caseSensitive, setCaseSensitive] = useState(false);
    const [regex, setRegex] = useState(false);
    const [wholeWord, setWholeWord] = useState(false);
    const [result, setResult] = useState<{ resultIndex: number; resultCount: number } | null>(null);

    useEffect(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
        const sub = runtime.onSearchResults(setResult);
        return () => {
            sub.dispose();
            runtime.clearSearch();
        };
    }, [runtime]);

    const find = (direction: 'next' | 'prev', q = query) => {
        runtime.find(q, direction, { caseSensitive, regex, wholeWord });
    };

    useEffect(() => {
        find('next', query);
        // 选项变了按当前词重搜
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [caseSensitive, regex, wholeWord]);

    const close = () => {
        onClose();
        runtime.focus();
    };

    const count = result
        ? result.resultCount === 0
            ? '无结果'
            : result.resultIndex < 0
              ? `${result.resultCount}+`
              : `${result.resultIndex + 1}/${result.resultCount}`
        : '';

    return (
        <div className="ncd-term-drop-down absolute right-4 top-2 z-20 flex items-center gap-1 rounded-sm border border-border bg-elevated px-1.5 py-1 shadow-popover">
            <input
                ref={inputRef}
                value={query}
                onChange={(e) => {
                    setQuery(e.target.value);
                    find('next', e.target.value);
                }}
                onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                        e.preventDefault();
                        find(e.shiftKey ? 'prev' : 'next');
                    } else if (e.key === 'Escape') {
                        e.preventDefault();
                        close();
                    }
                }}
                placeholder="搜索输出"
                spellCheck={false}
                className="h-6 w-44 rounded-xs bg-field px-2 text-[12px] text-text outline-none placeholder:text-text-tertiary"
            />
            <span className="w-12 text-center text-[11px] tabular-nums text-text-tertiary">{count}</span>
            <Toggle active={caseSensitive} onClick={() => setCaseSensitive((v) => !v)} title="区分大小写">
                <CaseSensitive size={14} />
            </Toggle>
            <Toggle active={wholeWord} onClick={() => setWholeWord((v) => !v)} title="全字匹配">
                <WholeWord size={14} />
            </Toggle>
            <Toggle active={regex} onClick={() => setRegex((v) => !v)} title="正则">
                <Regex size={14} />
            </Toggle>
            <Toggle active={false} onClick={() => find('prev')} title="上一个（Shift+Enter）">
                <ArrowUp size={14} />
            </Toggle>
            <Toggle active={false} onClick={() => find('next')} title="下一个（Enter）">
                <ArrowDown size={14} />
            </Toggle>
            <Toggle active={false} onClick={close} title="关闭（Esc）">
                <X size={14} />
            </Toggle>
        </div>
    );
}
