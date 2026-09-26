// 提示词编辑区的零件：参数标签、和默认的对比、版本菜单、版本预览、另存为新版本。

import { useMemo } from 'react';
import { History, Trash2 } from 'lucide-react';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Popover,
    PopoverClose,
    PopoverContent,
    PopoverTrigger,
    Spinner,
    SyntaxTextEditor,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { lineDiff, type DiffLine } from '../../../../core/domain/apps/textDiff';
import type { PromptCheck } from '../../../../core/domain/apps/maibotPrompts';
import type { MaiBotPromptVersion } from '../../../../core/ipc/types';

/** 秒级时间戳换成「3 分钟前」；0 是没记录 */
export function relativeTime(secs: number): string {
    if (!secs) return '';
    const diff = Date.now() / 1000 - secs;
    if (diff < 60) return '刚刚';
    if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
    if (diff < 86400 * 30) return `${Math.floor(diff / 86400)} 天前`;
    return new Date(secs * 1000).toLocaleDateString('zh-CN');
}

/** 默认模板要的参数：在的、缺的、多出来的一眼分开；点一下插到光标处 */
export const ParamChips: React.FC<{
    params: readonly string[];
    check: PromptCheck;
    onInsert: (param: string) => void;
}> = ({ params, check, onInsert }) => {
    if (params.length === 0 && check.extra.length === 0) {
        return <p className="text-xs text-text-tertiary">这个模板不用参数</p>;
    }
    const missing = new Set(check.missing);
    return (
        <div className="flex flex-wrap items-center gap-1.5">
            <span className="mr-0.5 text-xs text-text-tertiary">参数</span>
            {params.map((p) => (
                <button
                    key={p}
                    type="button"
                    title={missing.has(p) ? '模板里少了这个参数，点一下插到光标处' : '点一下插到光标处'}
                    onClick={() => onInsert(`{${p}}`)}
                    className={cn(
                        'inline-flex h-5 items-center rounded-pill border px-1.5 font-mono text-[10.5px] transition-colors',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
                        missing.has(p)
                            ? 'border-dashed border-danger/60 text-danger hover:bg-danger-soft'
                            : 'border-transparent bg-brand/[0.07] text-brand/90 hover:bg-brand/15 hover:text-brand',
                    )}
                >
                    {`{${p}}`}
                </button>
            ))}
            {check.extra.map((p) => (
                <span
                    key={`extra-${p}`}
                    title="默认模板里没有这个参数，麦麦给不出它的值"
                    className="inline-flex h-5 items-center rounded-pill bg-danger-soft px-1.5 font-mono text-[10.5px] text-danger line-through"
                >
                    {`{${p}}`}
                </span>
            ))}
        </div>
    );
};

const DIFF_LOOK: Record<DiffLine['kind'], { mark: string; line: string }> = {
    same: { mark: '', line: 'text-text-secondary' },
    add: { mark: '+', line: 'bg-success/10 text-text' },
    del: { mark: '−', line: 'bg-danger/10 text-text-tertiary' },
};

/** 默认内容和现在的内容逐行对比，删的在前、加的在后 */
export const DiffView: React.FC<{ before: string; after: string }> = ({ before, after }) => {
    const diff = useMemo(() => lineDiff(before, after), [before, after]);
    return (
        <div className="relative h-0 min-h-0 w-full flex-1 overflow-auto rounded-sm border border-border-subtle bg-field py-3">
            {diff.map((d, i) => (
                <div key={i} className={cn('flex text-[13px] leading-[1.8]', DIFF_LOOK[d.kind].line)}>
                    <span
                        aria-hidden
                        className={cn(
                            'w-7 shrink-0 select-none text-center font-mono',
                            d.kind === 'add' ? 'text-success' : d.kind === 'del' ? 'text-danger' : 'text-transparent',
                        )}
                    >
                        {DIFF_LOOK[d.kind].mark || '·'}
                    </span>
                    <span className={cn('min-w-0 flex-1 whitespace-pre-wrap break-words pr-4', d.kind === 'del' && 'line-through decoration-danger/40')}>
                        {d.text}
                    </span>
                </div>
            ))}
        </div>
    );
};

export const VersionMenu: React.FC<{
    versions: readonly MaiBotPromptVersion[];
    busy: boolean;
    canSaveAs: boolean;
    onPreview: (v: MaiBotPromptVersion) => void;
    onActivate: (v: MaiBotPromptVersion) => void;
    onDelete: (v: MaiBotPromptVersion) => void;
    onSaveAs: () => void;
}> = ({ versions, busy, canSaveAs, onPreview, onActivate, onDelete, onSaveAs }) => (
    <Popover>
        <PopoverTrigger asChild>
            <Button size="sm" variant="ghost" className="gap-1.5 px-2">
                <History size={13} />
                版本
                {versions.length > 0 && (
                    <span className="rounded-pill bg-inset px-1.5 font-mono text-2xs text-text-tertiary">{versions.length}</span>
                )}
            </Button>
        </PopoverTrigger>
        <PopoverContent align="end" sideOffset={6} className="w-80 p-0">
            <div className="border-b border-border-subtle px-3 py-2 text-xs font-medium text-text-secondary">版本记录</div>
            {versions.length === 0 ? (
                <p className="px-3 py-4 text-xs leading-relaxed text-text-tertiary">还没有版本。每次保存都会记一版，改坏了能换回来。</p>
            ) : (
                <ul className="max-h-72 overflow-y-auto p-1">
                    {versions.map((v) => (
                        <li key={v.id} className="group flex items-center gap-2 rounded-sm px-2 py-1.5 hover:bg-inset">
                            {/* 选了哪个动作都先收起菜单，不然会挂在预览 / 确认框后面 */}
                            <PopoverClose asChild>
                                <button
                                    type="button"
                                    className="min-w-0 flex-1 text-left focus-visible:outline-none"
                                    onClick={() => onPreview(v)}
                                >
                                    <span className="flex min-w-0 items-center gap-1.5">
                                        <span className="truncate text-[13px] text-text">{v.label}</span>
                                        {v.active && (
                                            <Badge tone="brand" className="shrink-0">
                                                在用
                                            </Badge>
                                        )}
                                    </span>
                                    <span className="block text-2xs text-text-tertiary">
                                        {relativeTime(v.modified_at) || v.id}
                                    </span>
                                </button>
                            </PopoverClose>
                            {!v.active && (
                                <PopoverClose asChild>
                                    <Button
                                        size="sm"
                                        variant="ghost"
                                        disabled={busy}
                                        className="h-6 px-2 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                                        onClick={() => onActivate(v)}
                                    >
                                        用这个
                                    </Button>
                                </PopoverClose>
                            )}
                            <PopoverClose asChild>
                                <button
                                    type="button"
                                    aria-label={`删掉版本 ${v.label}`}
                                    disabled={busy}
                                    onClick={() => onDelete(v)}
                                    className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-xs text-text-tertiary opacity-0 hover:bg-danger-soft hover:text-danger focus-visible:opacity-100 group-hover:opacity-100 disabled:opacity-40"
                                >
                                    <Trash2 size={12} />
                                </button>
                            </PopoverClose>
                        </li>
                    ))}
                </ul>
            )}
            <div className="border-t border-border-subtle p-1">
                <PopoverClose asChild>
                    <Button
                        size="sm"
                        variant="ghost"
                        className="w-full justify-start"
                        disabled={!canSaveAs || busy}
                        onClick={onSaveAs}
                    >
                        把现在的内容另存为新版本…
                    </Button>
                </PopoverClose>
            </div>
        </PopoverContent>
    </Popover>
);

export const VersionPreview: React.FC<{
    version: MaiBotPromptVersion | null;
    content: string | undefined;
    loading: boolean;
    busy: boolean;
    onClose: () => void;
    onActivate: (v: MaiBotPromptVersion) => void;
}> = ({ version, content, loading, busy, onClose, onActivate }) => (
    <Dialog open={!!version} onOpenChange={(o) => !o && onClose()}>
        <DialogContent size="lg">
            <DialogHeader>
                <DialogTitle>{version?.label}</DialogTitle>
                <p className="text-xs text-text-tertiary">
                    {[version && relativeTime(version.modified_at), version?.active ? '在用' : null].filter(Boolean).join(' · ')}
                </p>
            </DialogHeader>
            <div className="flex h-[52vh] min-h-0 flex-col">
                {loading || content === undefined ? (
                    <div className="flex flex-1 items-center justify-center">
                        <Spinner size="md" tone="brand" label="正在读取版本" />
                    </div>
                ) : (
                    <SyntaxTextEditor
                        value={content}
                        onChange={() => {}}
                        mode="prompt"
                        wrap
                        prose
                        disabled
                        aria-label="版本内容"
                        className="opacity-100"
                    />
                )}
            </div>
            <DialogFooter>
                <Button size="sm" variant="ghost" onClick={onClose}>
                    关闭
                </Button>
                {version && !version.active && (
                    <Button size="sm" variant="primary" disabled={busy || content === undefined} onClick={() => onActivate(version)}>
                        用这个版本
                    </Button>
                )}
            </DialogFooter>
        </DialogContent>
    </Dialog>
);
