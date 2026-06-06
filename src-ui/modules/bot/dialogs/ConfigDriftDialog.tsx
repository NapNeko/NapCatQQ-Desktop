// Config drift dialog: field-aware UI for resolving external config changes.
// Uses drift-display adapter to translate raw JSON into human-readable cards.

import { useMemo, useState } from 'react';
import { AlertTriangle, Plus, Pencil, Check, Wifi, WifiOff } from 'lucide-react';
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
    const allDecided = drift.modified.every((e) => modChoice[ek(e)] != null);
    const remaining = drift.modified.filter((e) => !modChoice[ek(e)]).length;

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
            <DialogContent size="sheet">
                <DialogHeader>
                    <DialogTitle className="flex items-center gap-2">
                        <ActionMotionIcon
                            icon={AlertTriangle}
                            size={18}
                            motion={infoToneMotion('warning')}
                            className="text-warning"
                        />
                        配置冲突
                    </DialogTitle>
                    <DialogDescription>
                        Bot {drift.bot_id} 的运行时文件与桌面端配置不一致，请逐项选择保留哪一方。
                    </DialogDescription>
                </DialogHeader>

                <div className="flex-1 overflow-y-auto space-y-6 py-3 scrollbar-hide">
                    {modifiedDisplay.length > 0 && (
                        <section>
                            <SectionHeader
                                icon={
                                    <ActionMotionIcon
                                        icon={Pencil}
                                        size={14}
                                        motion={infoToneMotion('warning')}
                                        className="text-warning"
                                    />
                                }
                                iconClass="text-warning"
                                title="值冲突"
                                count={modifiedDisplay.length}
                                tone="warning"
                            />
                            <div className="space-y-3 mt-3">
                                {modifiedDisplay.map((item) => (
                                    <ModifiedCard
                                        key={item.key}
                                        item={item}
                                        choice={modChoice[item.key] ?? null}
                                        onChoose={(v) => setModChoice((m) => ({ ...m, [item.key]: v }))}
                                    />
                                ))}
                            </div>
                        </section>
                    )}

                    {drift.added.length > 0 && (
                        <section>
                            <SectionHeader
                                icon={
                                    <ActionMotionIcon
                                        icon={Plus}
                                        size={14}
                                        motion={EMPHASIS_MOTION}
                                        className="text-success"
                                    />
                                }
                                iconClass="text-success"
                                title="新增字段"
                                count={drift.added.length}
                                tone="success"
                            />
                            <p className="text-2xs text-text-tertiary mt-1 mb-2">
                                运行时文件中多出的字段，桌面端不管理。默认保留，关掉则启动时移除。
                            </p>
                            <div className="space-y-1">
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

                <DialogFooter>
                    <Button variant="ghost" size="sm" onClick={onCancel}>取消启动</Button>
                    <Button variant="primary" size="sm" disabled={!allDecided} onClick={handleConfirm}>
                        {allDecided ? '应用并启动' : `还有 ${remaining} 项未选`}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}


// ─── Section header ──────────────────────────────────────────────────────────

function SectionHeader({ icon, iconClass, title, count, tone }: {
    icon: React.ReactNode;
    iconClass: string;
    title: string;
    count: number;
    tone: 'warning' | 'success';
}) {
    return (
        <div className="flex items-center gap-2">
            <span className={iconClass}>{icon}</span>
            <span className="text-sm font-medium text-text">{title}</span>
            <Badge tone={tone} appearance="soft">{count}</Badge>
        </div>
    );
}

// ─── Modified card ───────────────────────────────────────────────────────────

function ModifiedCard({ item, choice, onChoose }: {
    item: DriftDisplayEntry;
    choice: 'external' | 'internal' | null;
    onChoose: (v: 'external' | 'internal') => void;
}) {
    return (
        <div className="rounded-md ring-1 ring-border-subtle overflow-hidden">
            <div className="flex items-center gap-2 bg-inset px-3 py-1.5 border-b border-border-subtle">
                <span className="text-xs font-medium text-text">{item.label}</span>
                <span className="text-2xs text-text-tertiary font-mono ml-auto truncate max-w-[200px]">
                    {item.file}
                </span>
            </div>
            <div className="grid grid-cols-2 divide-x divide-border-subtle">
                <DiffPanel
                    label="桌面端"
                    value={item.ours}
                    selected={choice === 'internal'}
                    onClick={() => onChoose('internal')}
                    tone="green"
                />
                <DiffPanel
                    label="运行时文件"
                    value={item.theirs}
                    selected={choice === 'external'}
                    onClick={() => onChoose('external')}
                    tone="rose"
                />
            </div>
        </div>
    );
}

// ─── Diff panel ──────────────────────────────────────────────────────────────

function DiffPanel({ label, value, selected, onClick, tone }: {
    label: string;
    value: DriftDisplayValue;
    selected: boolean;
    onClick: () => void;
    tone: 'green' | 'rose';
}) {
    const bg = tone === 'green' ? 'bg-success-soft/30' : 'bg-danger-soft/30';
    const ring = tone === 'green' ? 'ring-success/50' : 'ring-danger/50';

    return (
        <button
            type="button"
            onClick={onClick}
            className={cn(
                'flex flex-col gap-1.5 px-3 py-2.5 text-left transition-all',
                'hover:bg-inset/40',
                selected && `${bg} ring-2 ${ring}`,
            )}
        >
            <div className="flex items-center justify-between">
                <span className="text-2xs font-medium text-text-secondary">{label}</span>
                {selected && (
                    <span className={cn(
                        'inline-flex items-center gap-0.5 text-2xs font-medium',
                        tone === 'green' ? 'text-success' : 'text-danger',
                    )}>
                        <Check size={10} strokeWidth={3} /> 选用
                    </span>
                )}
            </div>
            <ValueDisplay value={value} />
        </button>
    );
}

// ─── Value display: renders based on kind ────────────────────────────────────

function ValueDisplay({ value }: { value: DriftDisplayValue }) {
    switch (value.kind) {
        case 'scalar':
            return <span className="text-sm text-text">{value.text}</span>;
        case 'connections':
            return <ConnectionList items={value.items} />;
        case 'json':
            return (
                <pre className="text-2xs font-mono text-text-secondary whitespace-pre-wrap break-all max-h-28 overflow-y-auto leading-relaxed bg-canvas/50 rounded-xs px-2 py-1">
                    {value.preview}
                </pre>
            );
    }
}

// ─── Connection list ─────────────────────────────────────────────────────────

function ConnectionList({ items }: { items: ConnectionSummary[] }) {
    if (items.length === 0) {
        return <span className="text-2xs text-text-tertiary italic">(无连接)</span>;
    }
    return (
        <div className="flex flex-col gap-1">
            {items.map((c, i) => (
                <div key={i} className="flex items-center gap-2 rounded-xs bg-canvas/50 px-2 py-1">
                    {c.enabled
                        ? (
                            <ActionMotionIcon
                                icon={Wifi}
                                size={11}
                                motion={LIVE_MOTION}
                                className="text-success shrink-0"
                            />
                        )
                        : (
                            <ActionMotionIcon
                                icon={WifiOff}
                                size={11}
                                className="text-text-disabled shrink-0"
                            />
                        )}
                    <div className="flex flex-col min-w-0">
                        <span className="text-2xs font-medium text-text truncate">
                            {c.name}
                            <span className="ml-1.5 text-text-tertiary font-normal">{c.type}</span>
                        </span>
                        <span className="text-2xs font-mono text-text-tertiary truncate">
                            {c.endpoint || '(无端点)'}
                        </span>
                    </div>
                    {c.token !== '(无)' && (
                        <span className="ml-auto text-2xs font-mono text-text-tertiary shrink-0">
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
        <div className="flex items-center justify-between gap-3 rounded-sm bg-field px-3 py-2 ring-1 ring-border-subtle">
            <div className="flex min-w-0 flex-col gap-0.5">
                <span className="text-xs text-text truncate">
                    <span className="font-mono font-medium">{entry.path}</span>
                </span>
                <span className="text-2xs font-mono text-text-tertiary truncate">
                    {briefValue(entry.external)}
                </span>
            </div>
            <Switch checked={keep} onCheckedChange={onToggle} label="" />
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
    if (typeof v === 'boolean') return v ? '开启' : '关闭';
    if (typeof v === 'number') return String(v);
    const s = JSON.stringify(v);
    return s.length > 60 ? s.slice(0, 57) + '...' : s;
}
