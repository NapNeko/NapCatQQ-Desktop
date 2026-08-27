// Config drift dialog: field-aware UI for resolving external config changes.
// Uses drift-display adapter to translate raw JSON into human-readable cards.

import { useMemo, useState } from 'react';
import {
    AlertTriangle,
    Plus,
    Check,
    Wifi,
    WifiOff,
    CheckCircle2,
    Laptop,
    FileCode,
} from 'lucide-react';
import {
    ActionMotionIcon,
    EMPHASIS_MOTION,
    LIVE_MOTION,
    infoToneMotion,
} from '../../../shared/ui/motion';
import {
    Button,
    Badge,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
    Switch,
} from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import type { ConfigDrift } from '../../../core/ipc/generated/ConfigDrift';
import type { DriftEntry } from '../../../core/ipc/generated/DriftEntry';
import type { DriftDecision } from '../../../core/ipc/generated/DriftDecision';
import {
    transformDriftEntries,
    type DriftDisplayEntry,
    type DriftDisplayValue,
    type ConnectionSummary,
} from '../../../core/domain/bot/drift-display';

interface ConfigDriftDialogProps {
    open: boolean;
    drift: ConfigDrift;
    onConfirm: (decisions: DriftDecision[]) => void;
    onCancel: () => void;
}

export function ConfigDriftDialog({ open, drift, onConfirm, onCancel }: ConfigDriftDialogProps) {
    const modifiedDisplay = useMemo(() => transformDriftEntries(drift.modified), [drift.modified]);

    const [addedKeep, setAddedKeep] = useState<Record<string, boolean>>(() => {
        const m: Record<string, boolean> = {};
        for (const e of drift.added) m[ek(e)] = true;
        return m;
    });

    const [modChoice, setModChoice] = useState<Record<string, 'external' | 'internal'>>({});
    const totalModified = drift.modified.length;
    const decidedCount = drift.modified.filter((e) => modChoice[ek(e)] != null).length;
    const allDecided = decidedCount === totalModified;
    const remaining = totalModified - decidedCount;

    // 快捷批量操作
    const chooseAllInternal = () => {
        const next: Record<string, 'external' | 'internal'> = {};
        for (const e of drift.modified) next[ek(e)] = 'internal';
        setModChoice(next);
    };

    const chooseAllExternal = () => {
        const next: Record<string, 'external' | 'internal'> = {};
        for (const e of drift.modified) next[ek(e)] = 'external';
        setModChoice(next);
    };

    const toggleAllAdded = (keep: boolean) => {
        const next: Record<string, boolean> = {};
        for (const e of drift.added) next[ek(e)] = keep;
        setAddedKeep(next);
    };

    const handleConfirm = () => {
        const d: DriftDecision[] = [];
        for (const e of drift.added) {
            const k = ek(e);
            d.push(addedKeep[k]
                ? { kind: 'keep_added', file: e.file, path: e.path }
                : { kind: 'drop_added', file: e.file, path: e.path });
        }
        for (const e of drift.modified) {
            const k = ek(e);
            d.push(modChoice[k] === 'external'
                ? { kind: 'accept_external', file: e.file, path: e.path, value: e.external }
                : { kind: 'use_internal', file: e.file, path: e.path });
        }
        onConfirm(d);
    };

    return (
        <Dialog open={open} onOpenChange={(o) => { if (!o) onCancel(); }}>
            <DialogContent size="sheet" className="max-w-3xl max-h-[85vh] flex flex-col">
                <DialogHeader>
                    <DialogTitle className="flex items-center gap-2">
                        <ActionMotionIcon
                            icon={AlertTriangle}
                            size={18}
                            motion={infoToneMotion('warning')}
                            className="text-warning"
                        />
                        配置冲突解决
                    </DialogTitle>
                    <DialogDescription>
                        检测到 Bot {drift.bot_id} 运行时磁盘文件与桌面端配置存在差异。请选择每项要保留的值。
                    </DialogDescription>
                </DialogHeader>

                {/* 扁平化顶部工具栏 */}
                {totalModified > 0 && (
                    <div className="flex items-center justify-between border-b border-border-subtle/80 pb-2.5 pt-0.5 text-xs">
                        <div className="flex items-center gap-2">
                            <span className="font-medium text-text">
                                共 {totalModified} 处配置差异
                            </span>
                            {allDecided ? (
                                <Badge tone="success" appearance="soft" className="gap-1 font-normal">
                                    <Check size={11} strokeWidth={2.5} /> 全部已决断
                                </Badge>
                            ) : (
                                <Badge tone="warning" appearance="soft" className="font-normal">
                                    已选 {decidedCount}/{totalModified}
                                </Badge>
                            )}
                        </div>
                        <div className="flex items-center gap-2">
                            <Button
                                variant="secondary"
                                size="sm"
                                onClick={chooseAllInternal}
                                className="h-7 text-2xs gap-1.5 rounded-sm"
                            >
                                <Laptop size={12} className="opacity-75" />
                                全部选桌面端
                            </Button>
                            <Button
                                variant="secondary"
                                size="sm"
                                onClick={chooseAllExternal}
                                className="h-7 text-2xs gap-1.5 rounded-sm"
                            >
                                <FileCode size={12} className="opacity-75" />
                                全部选运行时
                            </Button>
                        </div>
                    </div>
                )}

                {/* 滚动容器增加 px-1.5 避免选中态 ring 裁剪 */}
                <div className="flex-1 overflow-y-auto px-1.5 py-1 scrollbar-hide divide-y divide-border-subtle/60">
                    {/* 差异列表（扁平分割线，无外框嵌套） */}
                    {modifiedDisplay.map((item) => (
                        <FlatConflictRow
                            key={item.key}
                            item={item}
                            choice={modChoice[item.key] ?? null}
                            onChoose={(v) => setModChoice((m) => ({ ...m, [item.key]: v }))}
                        />
                    ))}

                    {/* 运行时新增字段 */}
                    {drift.added.length > 0 && (
                        <section className="space-y-2.5 pt-4 pb-2">
                            <div className="flex items-center justify-between gap-2">
                                <div className="flex items-center gap-2">
                                    <ActionMotionIcon
                                        icon={Plus}
                                        size={14}
                                        motion={EMPHASIS_MOTION}
                                        className="text-success"
                                    />
                                    <span className="text-xs font-semibold text-text">运行时新增字段</span>
                                    <Badge tone="success" appearance="soft">
                                        {drift.added.length}
                                    </Badge>
                                </div>
                                <div className="flex items-center gap-1.5">
                                    <Button
                                        variant="ghost"
                                        size="sm"
                                        onClick={() => toggleAllAdded(true)}
                                        className="h-6 text-2xs text-text-secondary px-2 rounded-sm"
                                    >
                                        全部保留
                                    </Button>
                                    <span className="text-border-strong select-none">/</span>
                                    <Button
                                        variant="ghost"
                                        size="sm"
                                        onClick={() => toggleAllAdded(false)}
                                        className="h-6 text-2xs text-text-secondary px-2 rounded-sm"
                                    >
                                        全部移除
                                    </Button>
                                </div>
                            </div>
                            <p className="text-2xs text-text-tertiary">
                                运行时文件中检测到的未纳管字段。开启则保留写入，关闭则启动时自动清理。
                            </p>
                            <div className="space-y-1.5 pt-1">
                                {drift.added.map((entry) => (
                                    <AddedRow
                                        key={ek(entry)}
                                        entry={entry}
                                        keep={addedKeep[ek(entry)] ?? true}
                                        onToggle={(v) => setAddedKeep((m) => ({ ...m, [ek(entry)]: v }))}
                                    />
                                ))}
                            </div>
                        </section>
                    )}
                </div>

                <DialogFooter className="flex items-center justify-between sm:justify-between border-t border-border-subtle pt-3">
                    <div className="text-xs text-text-secondary">
                        {allDecided ? (
                            <span className="inline-flex items-center gap-1 text-success font-medium">
                                <CheckCircle2 size={14} /> 所有冲突已就绪
                            </span>
                        ) : (
                            <span>
                                剩余 <strong className="text-warning font-semibold">{remaining}</strong> 项待选择
                            </span>
                        )}
                    </div>
                    <div className="flex items-center gap-2">
                        <Button variant="secondary" size="sm" onClick={onCancel} className="rounded-sm">
                            取消启动
                        </Button>
                        <Button
                            variant="primary"
                            size="sm"
                            disabled={!allDecided}
                            onClick={handleConfirm}
                            className="rounded-sm"
                        >
                            应用并启动 Bot
                        </Button>
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}

// ─── Flat Conflict Row ───────────────────────────────────────────────────────

function FlatConflictRow({
    item,
    choice,
    onChoose,
}: {
    item: DriftDisplayEntry;
    choice: 'external' | 'internal' | null;
    onChoose: (v: 'external' | 'internal') => void;
}) {
    return (
        <div className="py-3.5 space-y-2">
            {/* 字段纯净元信息行 */}
            <div className="flex items-center justify-between text-xs px-0.5">
                <div className="flex items-baseline gap-2 min-w-0">
                    <span className="font-semibold text-text text-sm">
                        {item.label}
                    </span>
                    <code
                        className="rounded-xs bg-field/80 px-1.5 py-0.5 font-mono text-[11px] text-text-tertiary border border-border-subtle/50 truncate max-w-[240px]"
                        title={item.path}
                    >
                        {item.path}
                    </code>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                    <span className="font-mono text-2xs text-text-tertiary truncate max-w-[180px]" title={item.file}>
                        {item.file}
                    </span>
                    {choice === 'internal' && (
                        <Badge tone="brand" appearance="soft" className="text-2xs">
                            已选桌面端
                        </Badge>
                    )}
                    {choice === 'external' && (
                        <Badge tone="info" appearance="soft" className="text-2xs">
                            已选运行时
                        </Badge>
                    )}
                    {choice === null && (
                        <Badge tone="warning" appearance="outline" className="text-2xs">
                            待选择
                        </Badge>
                    )}
                </div>
            </div>

            {/* 单层扁平化二选一按钮 */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
                {/* 桌面端选项 */}
                <FlatOptionButton
                    label="桌面端配置"
                    selected={choice === 'internal'}
                    onClick={() => onChoose('internal')}
                    value={item.ours}
                />

                {/* 运行时选项 */}
                <FlatOptionButton
                    label="运行时文件"
                    selected={choice === 'external'}
                    onClick={() => onChoose('external')}
                    value={item.theirs}
                />
            </div>
        </div>
    );
}

// ─── Flat Option Button ──────────────────────────────────────────────────────

function FlatOptionButton({
    label,
    selected,
    onClick,
    value,
}: {
    label: string;
    selected: boolean;
    onClick: () => void;
    value: DriftDisplayValue;
}) {
    return (
        <button
            type="button"
            onClick={onClick}
            className={cn(
                'group flex flex-col justify-between rounded-md border p-3 text-left transition-all cursor-pointer select-none',
                selected
                    ? 'border-brand bg-brand-soft ring-1 ring-brand/35 text-text shadow-xs'
                    : 'border-border-subtle/80 bg-field/40 hover:border-border hover:bg-field/80 text-text-secondary',
            )}
        >
            {/* 头部：单选圆点 + 标签 + 选中角标 */}
            <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                    <div
                        className={cn(
                            'flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-pill border transition-all',
                            selected
                                ? 'border-brand bg-brand text-white shadow-xs'
                                : 'border-border-strong bg-transparent group-hover:border-border-strong',
                        )}
                    >
                        {selected && <Check size={9} strokeWidth={3} />}
                    </div>
                    <span className={cn(
                        'text-xs font-medium transition-colors',
                        selected ? 'text-brand font-semibold' : 'text-text-secondary group-hover:text-text'
                    )}>
                        {label}
                    </span>
                </div>

                {selected ? (
                    <span className="text-[11px] font-semibold text-brand inline-flex items-center gap-0.5">
                        ✓ 已选
                    </span>
                ) : (
                    <span className="text-[11px] text-text-tertiary opacity-0 group-hover:opacity-100 transition-opacity">
                        点击选用
                    </span>
                )}
            </div>

            {/* 值展示区 */}
            <div className="min-w-0 flex-1 flex flex-col justify-center">
                <ValueDisplay value={value} />
            </div>
        </button>
    );
}

// ─── Value display: renders based on kind ────────────────────────────────────

function ValueDisplay({ value }: { value: DriftDisplayValue }) {
    switch (value.kind) {
        case 'scalar':
            if (value.text === '(空)') {
                return (
                    <span className="text-xs font-mono text-text-disabled italic">
                        (空 / 未配置)
                    </span>
                );
            }
            if (value.text === '开启') {
                return <Badge tone="success" appearance="soft">true · 开启</Badge>;
            }
            if (value.text === '关闭') {
                return <Badge tone="neutral" appearance="soft">false · 关闭</Badge>;
            }
            return (
                <div className="inline-flex items-center rounded-xs bg-canvas/80 px-2 py-0.5 font-mono text-xs font-medium text-text border border-border-subtle/60 break-all">
                    {value.text}
                </div>
            );
        case 'connections':
            return <ConnectionList items={value.items} />;
        case 'json':
            return (
                <pre className="rounded-xs bg-canvas/70 p-2 font-mono text-[11px] leading-relaxed text-text-secondary max-h-28 overflow-y-auto whitespace-pre-wrap break-all border border-border-subtle/50 scrollbar-hide">
                    {value.preview}
                </pre>
            );
    }
}

// ─── Connection list ─────────────────────────────────────────────────────────

function ConnectionList({ items }: { items: ConnectionSummary[] }) {
    if (items.length === 0) {
        return (
            <div className="text-2xs italic text-text-tertiary">
                (无活动连接配置)
            </div>
        );
    }
    return (
        <div className="flex flex-col gap-1">
            {items.map((c, i) => (
                <div
                    key={i}
                    className="flex flex-wrap items-center gap-1.5 text-2xs text-text"
                >
                    {c.enabled ? (
                        <ActionMotionIcon
                            icon={Wifi}
                            size={11}
                            motion={LIVE_MOTION}
                            className="text-success shrink-0"
                        />
                    ) : (
                        <ActionMotionIcon
                            icon={WifiOff}
                            size={11}
                            className="text-text-disabled shrink-0"
                        />
                    )}
                    <span className="font-medium truncate">{c.name}</span>
                    <span className="font-mono text-text-tertiary text-[10px]">({c.type})</span>
                    <span className="font-mono text-text-secondary truncate max-w-[160px]" title={c.endpoint}>
                        {c.endpoint || '(无端点)'}
                    </span>
                    {c.token && c.token !== '(无)' && (
                        <span className="font-mono text-[10px] text-text-tertiary ml-auto">
                            Token: {c.token}
                        </span>
                    )}
                </div>
            ))}
        </div>
    );
}

// ─── Added row ───────────────────────────────────────────────────────────────

function AddedRow({ entry, keep, onToggle }: {
    entry: DriftEntry;
    keep: boolean;
    onToggle: (v: boolean) => void;
}) {
    return (
        <div className="flex items-center justify-between gap-3 rounded-md border border-border-subtle/70 bg-field/40 px-3.5 py-2 transition-colors hover:bg-field/70">
            <div className="flex min-w-0 flex-col gap-0.5">
                <span className="text-xs font-mono font-medium text-text truncate">
                    {entry.path}
                </span>
                <span className="text-2xs font-mono text-text-tertiary truncate max-w-[420px]" title={briefValue(entry.external)}>
                    检测值: {briefValue(entry.external)}
                </span>
            </div>
            <div className="flex items-center gap-2 shrink-0">
                <span className="text-2xs text-text-secondary">
                    {keep ? '保留' : '丢弃'}
                </span>
                <Switch checked={keep} onCheckedChange={onToggle} label="" />
            </div>
        </div>
    );
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function ek(entry: DriftEntry): string {
    return `${entry.file}::${entry.path}`;
}

function briefValue(v: unknown): string {
    if (v === null || v === undefined) return 'null';
    if (typeof v === 'string') return v || '(空)';
    if (typeof v === 'boolean') return v ? 'true' : 'false';
    if (typeof v === 'number') return String(v);
    const s = JSON.stringify(v);
    return s.length > 60 ? s.slice(0, 57) + '...' : s;
}





