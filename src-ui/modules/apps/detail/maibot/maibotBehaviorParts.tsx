// 行为页的零件：一行一条经验（情境标签、做法 → 结果、分数、次数）、分数条、详情框（情境分量、反馈和观察记录）。

import type { ComponentType, ReactNode } from 'react';
import type { LucideProps } from 'lucide-react';
import {
    ArrowRight,
    Ban,
    Bot,
    CircleCheck,
    CircleDashed,
    CircleMinus,
    CircleX,
    Eye,
    TrendingDown,
    User,
    Users,
} from 'lucide-react';
import {
    Badge,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    Spinner,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import type {
    MaiBotBehavior,
    MaiBotBehaviorActor,
    MaiBotBehaviorDetail,
    MaiBotBehaviorEvidence,
    MaiBotBehaviorFeedback,
    MaiBotBehaviorFeedbackKind,
    MaiBotBehaviorTag,
    MaiBotBehaviorTagKind,
} from '../../../../core/ipc/types';
import { relativeTime } from './maibotPromptParts';

const KIND_LABEL: Readonly<Record<MaiBotBehaviorTagKind, string>> = {
    domain: '话题',
    need: '需求',
    attitude: '态度',
    other: '其他',
};

const KIND_DOT: Readonly<Record<MaiBotBehaviorTagKind, string>> = {
    domain: 'bg-info',
    need: 'bg-brand',
    attitude: 'bg-warning',
    other: 'bg-text-disabled',
};

const WHO: Readonly<Record<MaiBotBehaviorActor, string>> = {
    others: '别人',
    group: '大家',
    maibot: '麦麦',
    unknown: '有人',
};

/** 这条是怎么学到的：看别人做 / 麦麦自己试 */
export function behaviorOrigin(b: MaiBotBehavior): string {
    return b.self_reflection ? '麦麦自己试出来的' : `看${WHO[b.actor]}这么做学到的`;
}

const pct = (w: number) => `${Math.round(w * 100)}%`;

// 自动维护一次只扣 0.25、0.35 这种，一位小数会抹成 0.2、0.3
function signed(n: number): string {
    const r = Math.round(n * 100) / 100;
    if (r === 0) return '0';
    const s = Number.isInteger(Math.round(r * 100) / 10) ? r.toFixed(1) : r.toFixed(2);
    return r > 0 ? `+${s}` : s;
}

const ACTOR_ICON: Readonly<Record<MaiBotBehaviorActor, ComponentType<LucideProps>>> = {
    others: User,
    group: Users,
    maibot: Bot,
    unknown: User,
};

const ActorIcon: React.FC<{ b: MaiBotBehavior }> = ({ b }) => {
    const Icon = ACTOR_ICON[b.actor];
    return (
        <span
            title={behaviorOrigin(b)}
            className={cn(
                'mt-0.5 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full',
                b.self_reflection ? 'bg-brand-soft text-brand' : 'bg-inset text-text-secondary',
            )}
        >
            <Icon size={14} />
        </span>
    );
};

/** 情境标签：圆点颜色分话题 / 需求 / 态度，悬停看占多少 */
export const SceneTags: React.FC<{ tags: readonly MaiBotBehaviorTag[]; max?: number }> = ({
    tags,
    max,
}) => {
    const shown = max ? tags.slice(0, max) : tags;
    if (shown.length === 0)
        return <span className="text-2xs text-text-tertiary">情境没有归出名字</span>;
    return (
        <span className="flex min-w-0 flex-wrap items-center gap-1">
            {shown.map((t) => (
                <span
                    key={`${t.kind}:${t.label}`}
                    title={`${KIND_LABEL[t.kind]}，占 ${pct(t.weight)}`}
                    className="inline-flex max-w-[14rem] items-center gap-1 rounded-pill bg-inset px-2 py-0.5 text-2xs text-text-secondary"
                >
                    <span className={cn('h-1.5 w-1.5 shrink-0 rounded-full', KIND_DOT[t.kind])} />
                    <span className="truncate">{t.label}</span>
                </span>
            ))}
            {max !== undefined && tags.length > max && (
                <span className="text-2xs text-text-tertiary">+{tags.length - max}</span>
            )}
        </span>
    );
};

// 上游分数夹在 -6 到 8 之间；0 在条上偏左，往右绿、往左红
const MIN = -6;
const MAX = 8;
const ZERO = -MIN / (MAX - MIN);

/** wide 时条铺满外层剩下的宽度（详情框的统计格里） */
export const ScoreMeter: React.FC<{ score: number; wide?: boolean }> = ({ score, wide }) => {
    const v = (Math.min(MAX, Math.max(MIN, score)) - MIN) / (MAX - MIN);
    const good = score >= 1;
    const bad = score <= -1;
    return (
        <span
            className={cn('flex items-center gap-1.5', wide && 'w-full')}
            title={`分数 ${signed(score)}，范围 -6 到 8`}
        >
            <span
                className={cn(
                    'relative h-1.5 overflow-hidden rounded-full bg-inset',
                    wide ? 'min-w-0 flex-1' : 'w-12',
                )}
            >
                <span
                    className={cn(
                        'absolute inset-y-0',
                        good ? 'bg-success' : bad ? 'bg-danger' : 'bg-text-disabled',
                    )}
                    style={{
                        left: `${Math.min(ZERO, v) * 100}%`,
                        width: `${Math.max(Math.abs(v - ZERO) * 100, 3)}%`,
                    }}
                />
                <span
                    className="absolute inset-y-0 w-px bg-text-tertiary/50"
                    style={{ left: `${ZERO * 100}%` }}
                />
            </span>
            <span
                className={cn(
                    'min-w-[2.25rem] text-right font-mono text-xs tabular-nums',
                    good ? 'text-success' : bad ? 'text-danger' : 'text-text-secondary',
                )}
            >
                {signed(score)}
            </span>
        </span>
    );
};

export const BehaviorRow: React.FC<{
    item: MaiBotBehavior;
    showChat: boolean;
    onOpen: () => void;
}> = ({ item: b, showChat, onOpen }) => (
    <button
        type="button"
        onClick={onOpen}
        className="flex w-full items-start gap-3 rounded-md border border-border-subtle bg-surface px-3 py-2.5 text-left transition-colors hover:border-border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40"
    >
        <ActorIcon b={b} />
        <span className="min-w-0 flex-1">
            <SceneTags tags={b.scene} max={3} />
            <span
                className={cn(
                    'mt-1.5 block truncate text-[13.5px]',
                    b.enabled ? 'text-text' : 'text-text-tertiary',
                )}
            >
                {b.action}
            </span>
            <span className="mt-0.5 flex min-w-0 items-center gap-1 text-xs text-text-secondary">
                <ArrowRight size={12} className="shrink-0 text-text-tertiary" />
                <span className="truncate">{b.outcome}</span>
            </span>
            <span className="mt-1 flex flex-wrap gap-x-2.5 text-2xs text-text-tertiary">
                {showChat && <span className="max-w-[12rem] truncate">{b.chat_name}</span>}
                <span>见过 {b.seen} 次</span>
                {b.used > 0 && (
                    <span>
                        用过 {b.used} 次
                        {b.succeeded + b.failed > 0 && `，成 ${b.succeeded} 败 ${b.failed}`}
                    </span>
                )}
                {b.active_at !== undefined && <span>{relativeTime(b.active_at)}</span>}
            </span>
        </span>
        <span className="flex shrink-0 flex-col items-end gap-1.5 pt-0.5">
            <ScoreMeter score={b.score} />
            {!b.enabled && <Badge tone="neutral">已停用</Badge>}
        </span>
    </button>
);

const FEEDBACK: Readonly<
    Record<
        MaiBotBehaviorFeedbackKind,
        { label: string; icon: ComponentType<LucideProps>; className: string }
    >
> = {
    success: { label: '成了', icon: CircleCheck, className: 'text-success' },
    partial: { label: '成了一半', icon: CircleMinus, className: 'text-warning' },
    failure: { label: '没成', icon: CircleX, className: 'text-danger' },
    neutral: { label: '看不出好坏', icon: CircleDashed, className: 'text-text-tertiary' },
    decay: { label: '很久没用上，自动扣分', icon: TrendingDown, className: 'text-warning' },
    disabled: { label: '自动停用', icon: Ban, className: 'text-danger' },
};

const Section: React.FC<{ title: string; count?: number; children: ReactNode }> = ({
    title,
    count,
    children,
}) => (
    <section className="flex flex-col gap-2">
        <h4 className="text-xs font-medium text-text-secondary">
            {title}
            {count !== undefined && (
                <span className="ml-1.5 font-mono text-2xs text-text-tertiary">{count}</span>
            )}
        </h4>
        {children}
    </section>
);

const Block: React.FC<{ title: string; text: string }> = ({ title, text }) => (
    <div className="rounded-md bg-field px-3 py-2.5">
        <p className="text-2xs text-text-tertiary">{title}</p>
        <p className="mt-1 whitespace-pre-wrap text-[13px] leading-relaxed text-text">{text}</p>
    </div>
);

const Stat: React.FC<{ label: string; children: ReactNode }> = ({ label, children }) => (
    <div className="flex min-w-0 flex-col gap-1 rounded-md bg-inset/60 px-3 py-2">
        <span className="text-2xs text-text-tertiary">{label}</span>
        <span className="flex h-5 items-center text-sm font-medium tabular-nums text-text">
            {children}
        </span>
    </div>
);

const SceneBreakdown: React.FC<{ tags: readonly MaiBotBehaviorTag[] }> = ({ tags }) =>
    tags.length === 0 ? (
        <p className="text-xs text-text-tertiary">这类情境还没归出名字</p>
    ) : (
        <ul className="flex flex-col gap-1.5">
            {tags.map((t) => (
                <li
                    key={`${t.kind}:${t.label}`}
                    className="grid grid-cols-[3rem_minmax(0,1fr)_8rem] items-center gap-2 text-xs"
                >
                    <span className="flex items-center gap-1.5 text-text-tertiary">
                        <span
                            className={cn('h-1.5 w-1.5 shrink-0 rounded-full', KIND_DOT[t.kind])}
                        />
                        {KIND_LABEL[t.kind]}
                    </span>
                    <span className="truncate text-text">{t.label}</span>
                    <span className="flex items-center gap-1.5">
                        <span className="relative h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-inset">
                            <span
                                className={cn(
                                    'absolute inset-y-0 left-0 rounded-full',
                                    KIND_DOT[t.kind],
                                )}
                                style={{ width: pct(t.weight) }}
                            />
                        </span>
                        <span className="w-8 text-right font-mono text-2xs tabular-nums text-text-tertiary">
                            {pct(t.weight)}
                        </span>
                    </span>
                </li>
            ))}
        </ul>
    );

const join = (parts: readonly (string | false | undefined)[]) => parts.filter(Boolean).join(' · ');

const FeedbackList: React.FC<{ items: readonly MaiBotBehaviorFeedback[] }> = ({ items }) =>
    items.length === 0 ? (
        <p className="text-xs text-text-tertiary">
            还没有反馈。麦麦照着做过以后，会按对方的反应记一笔。
        </p>
    ) : (
        <ul className="flex flex-col">
            {items.map((f, i) => {
                const meta = FEEDBACK[f.kind];
                const Icon = meta.icon;
                return (
                    <li
                        key={i}
                        className="flex items-start gap-2 rounded-sm px-2 py-1.5 hover:bg-inset/60"
                    >
                        <Icon size={14} className={cn('mt-0.5 shrink-0', meta.className)} />
                        <span className="min-w-0 flex-1">
                            <span className="block text-xs leading-relaxed text-text">
                                {f.reason || meta.label}
                            </span>
                            <span className="mt-0.5 block text-2xs text-text-tertiary">
                                {join([
                                    !!f.reason && meta.label,
                                    !!f.outcome && `结果：${f.outcome}`,
                                    f.at !== undefined && relativeTime(f.at),
                                ])}
                            </span>
                        </span>
                        {f.delta !== 0 && (
                            <span
                                className={cn(
                                    'shrink-0 font-mono text-2xs tabular-nums',
                                    f.delta > 0 ? 'text-success' : 'text-danger',
                                )}
                            >
                                {signed(f.delta)}
                            </span>
                        )}
                    </li>
                );
            })}
        </ul>
    );

const EvidenceList: React.FC<{
    items: readonly MaiBotBehaviorEvidence[];
    item: MaiBotBehavior;
}> = ({ items, item }) =>
    items.length === 0 ? (
        <p className="text-xs text-text-tertiary">没留下观察记录</p>
    ) : (
        <ul className="flex flex-col">
            {items.map((e, i) => (
                <li
                    key={i}
                    className="flex items-start gap-2 rounded-sm px-2 py-1.5 hover:bg-inset/60"
                >
                    <Eye size={14} className="mt-0.5 shrink-0 text-text-tertiary" />
                    <span className="min-w-0 flex-1">
                        <span className="block text-xs text-text">
                            {e.actor === 'maibot'
                                ? '麦麦自己这么做了'
                                : `看到${WHO[e.actor]}这么做`}
                        </span>
                        {(e.action !== item.action || e.outcome !== item.outcome) && (
                            <span className="mt-0.5 block text-2xs leading-relaxed text-text-secondary">
                                {e.action} → {e.outcome}
                            </span>
                        )}
                        <span className="mt-0.5 block text-2xs text-text-tertiary">
                            {join([
                                e.messages > 0 && `依据 ${e.messages} 条消息`,
                                e.at !== undefined && relativeTime(e.at),
                            ])}
                        </span>
                    </span>
                </li>
            ))}
        </ul>
    );

/** 一条经验的全貌。item 是列表里那条，详情回来前先用它把上半截画出来 */
export const BehaviorDetailDialog: React.FC<{
    open: boolean;
    item: MaiBotBehavior | undefined;
    detail: MaiBotBehaviorDetail | undefined;
    loading: boolean;
    error: string | undefined;
    onClose: () => void;
}> = ({ open, item, detail, loading, error, onClose }) => {
    const b = detail?.item ?? item;
    const records = (render: (d: MaiBotBehaviorDetail) => ReactNode) =>
        detail ? (
            render(detail)
        ) : loading ? (
            <div className="flex h-16 items-center justify-center">
                <Spinner size="sm" tone="brand" label="正在读取" />
            </div>
        ) : (
            <p className="text-xs text-danger">{error ?? '没读到'}</p>
        );
    return (
        <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
            <DialogContent size="lg">
                <DialogHeader>
                    <DialogTitle className="leading-snug">{b?.action ?? '经验'}</DialogTitle>
                    {b && (
                        <DialogDescription>
                            {join([behaviorOrigin(b), b.chat_name])}
                        </DialogDescription>
                    )}
                </DialogHeader>
                {b && (
                    <div className="flex max-h-[62vh] flex-col gap-5 overflow-y-auto pr-1">
                        <Block title="结果怎样" text={b.outcome} />
                        <Section title="什么情境下">
                            <SceneBreakdown tags={b.scene} />
                        </Section>
                        <div className="flex flex-col gap-2">
                            <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                                <Stat label="分数">
                                    <ScoreMeter score={b.score} wide />
                                </Stat>
                                <Stat label="见过">{b.seen} 次</Stat>
                                <Stat label="照着做过">{b.used} 次</Stat>
                                <Stat label="成 / 败">
                                    {b.succeeded} / {b.failed}
                                </Stat>
                            </div>
                            <p className="text-2xs leading-relaxed text-text-tertiary">
                                {b.enabled
                                    ? '照着做了，对方反应好就加分、不好就扣分；分数低于 -4 就不会再被挑中。'
                                    : '麦麦已经不用这条了：分数掉到了底，或者很久都没用上。'}
                            </p>
                        </div>
                        <Section title="反馈" count={detail?.feedback.length}>
                            {records((d) => (
                                <FeedbackList items={d.feedback} />
                            ))}
                        </Section>
                        <Section title="观察" count={detail?.evidence.length}>
                            {records((d) => (
                                <EvidenceList items={d.evidence} item={b} />
                            ))}
                        </Section>
                    </div>
                )}
            </DialogContent>
        </Dialog>
    );
};
