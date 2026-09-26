// 知识库「图谱」：人和物是点、关系是线，只画连得最多的那批。点一个点看它牵连的关系和出处，
// 顺着关系点到下一个；搜名字能跳过去（不在画面里的也能打开看）。

import { useMemo, useState, type ReactNode } from 'react';
import { ArrowRight, Maximize2, Network, Trash2, X } from 'lucide-react';
import { Button, Spinner } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import { forceLayout } from '../../../../core/domain/apps/graphLayout';
import type { MaiBotMemoryNodeDetail } from '../../../../core/ipc/types';
import {
    useMaiBotMemoryGraph,
    useMaiBotMemoryGraphNode,
    useMaiBotMemoryGraphSearch,
} from '../../../../hooks/apps/useMaiBotMemory';
import { EmptyHint } from '../entityParts';
import { ResourcePane, SearchBox, Segmented, useDebounced } from '../resourceParts';
import { GraphCanvas } from './maibotKnowledgeGraphCanvas';
import { useMemoryDelete } from './maibotKnowledgeParts';

const SIZES = [
    { value: '60', label: '60' },
    { value: '120', label: '120' },
    { value: '240', label: '240' },
] as const;

export const KnowledgeGraph: React.FC<{ instanceId: string; switcher: ReactNode; onImport: () => void }> = ({
    instanceId,
    switcher,
    onImport,
}) => {
    const [size, setSize] = useState<'60' | '120' | '240'>('120');
    const [selected, setSelected] = useState<string | null>(null);
    const [centerOn, setCenterOn] = useState<string | null>(null);
    const [fitSignal, setFitSignal] = useState(0);
    const [search, setSearch] = useState('');
    const [searchOpen, setSearchOpen] = useState(false);
    const q = useDebounced(search.trim());

    const graph = useMaiBotMemoryGraph(instanceId, Number(size), true);
    const hits = useMaiBotMemoryGraphSearch(instanceId, q, searchOpen);
    const node = useMaiBotMemoryGraphNode(instanceId, selected);
    const del = useMemoryDelete(instanceId, () => setSelected(null));

    const data = graph.data;
    // 布局只在数据变了时算一次；几百个点几十毫秒
    const layout = useMemo(() => (data ? forceLayout(data.nodes, data.edges) : new Map()), [data]);
    const shown = new Set(data?.nodes.map((n) => n.id) ?? []);

    const jump = (id: string) => {
        setSelected(id);
        if (shown.has(id)) setCenterOn(id);
        setSearchOpen(false);
    };

    const toolbar = (
        <>
            {switcher}
            <div className="relative">
                <SearchBox
                    className="w-52"
                    placeholder="搜人、物或关系"
                    value={search}
                    onChange={(v) => {
                        setSearch(v);
                        setSearchOpen(true);
                    }}
                />
                {searchOpen && q && (
                    <div className="absolute left-0 top-9 z-20 w-72 overflow-hidden rounded-md bg-elevated shadow-popover ring-1 ring-border-subtle">
                        {hits.isFetching && !hits.data ? (
                            <p className="px-3 py-2 text-xs text-text-tertiary">正在找…</p>
                        ) : (hits.data ?? []).length === 0 ? (
                            <p className="px-3 py-2 text-xs text-text-tertiary">没找到</p>
                        ) : (
                            <ul className="max-h-64 overflow-y-auto py-1">
                                {(hits.data ?? []).map((h, i) => (
                                    <li key={i}>
                                        <button
                                            type="button"
                                            onClick={() => jump(h.node)}
                                            className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-text hover:bg-inset"
                                        >
                                            <span className="shrink-0 text-2xs text-text-tertiary">{h.kind === 'relation' ? '关系' : '实体'}</span>
                                            <span className="min-w-0 flex-1 truncate">{h.title}</span>
                                            {!shown.has(h.node) && <span className="shrink-0 text-2xs text-text-disabled">不在画面里</span>}
                                        </button>
                                    </li>
                                ))}
                            </ul>
                        )}
                    </div>
                )}
            </div>
            <span className="flex items-center gap-1.5 text-xs text-text-tertiary">
                画
                <Segmented items={SIZES} value={size} onChange={setSize} />
                个点
            </span>
            <span className="flex-1" />
            {data && (
                <span className="text-xs text-text-tertiary">
                    画了 {data.nodes.length} / 共 {data.total_nodes} 个点
                </span>
            )}
            <Button size="sm" variant="ghost" onClick={() => setFitSignal((n) => n + 1)} disabled={!data || data.nodes.length === 0}>
                <Maximize2 size={13} />
                看全图
            </Button>
        </>
    );

    return (
        <ResourcePane toolbar={toolbar} fill>
            {graph.isFetching && !data ? (
                <div className="flex h-full min-h-60 items-center justify-center">
                    <Spinner size="md" tone="brand" label="正在读取图谱" />
                </div>
            ) : !data || data.nodes.length === 0 ? (
                <EmptyHint
                    icon={Network}
                    title="还没有能画的关系。导资料时开着「用模型抽取人物和关系」，图谱就会长出来。"
                    action={
                        <Button size="sm" variant="secondary" onClick={onImport}>
                            去导入
                        </Button>
                    }
                />
            ) : (
                <div className="relative h-full min-h-[22rem]">
                    <GraphCanvas
                        graph={data}
                        layout={layout}
                        selected={selected}
                        onSelect={(id) => {
                            setSelected(id);
                            setSearchOpen(false);
                        }}
                        fitSignal={fitSignal}
                        centerOn={centerOn}
                    />
                    {selected && (
                        <NodePanel
                            id={selected}
                            detail={node.data}
                            loading={node.isFetching && !node.data}
                            busy={del.busy}
                            onGo={jump}
                            onDelete={(d) => d.hash && del.start({ kind: 'entity', ids: [d.hash] }, `「${d.id}」和它的关系`)}
                            onClose={() => setSelected(null)}
                        />
                    )}
                </div>
            )}
            {del.confirm}
        </ResourcePane>
    );
};

/** 右侧浮着的一张卡：这个点牵连的关系（另一头点了就跳过去）、出自哪些原文 */
const NodePanel: React.FC<{
    id: string;
    detail: MaiBotMemoryNodeDetail | undefined;
    loading: boolean;
    busy: boolean;
    onGo: (id: string) => void;
    onDelete: (d: MaiBotMemoryNodeDetail) => void;
    onClose: () => void;
}> = ({ id, detail, loading, busy, onGo, onDelete, onClose }) => (
    <aside className="absolute bottom-3 right-3 top-3 flex w-80 max-w-[60%] flex-col overflow-hidden rounded-md bg-elevated shadow-popover ring-1 ring-border-subtle">
        <header className="flex items-start gap-2 border-b border-border-subtle px-4 py-3">
            <div className="min-w-0 flex-1">
                <h3 className="truncate font-display text-md font-semibold text-text">{id}</h3>
                {detail && (
                    <p className="mt-0.5 text-2xs text-text-tertiary">
                        {detail.relations.length} 条关系 · 出现在 {detail.mentions || detail.paragraphs.length} 段里
                    </p>
                )}
            </div>
            <Button size="icon" variant="ghost" className="h-7 w-7" aria-label="关掉" onClick={onClose}>
                <X size={14} />
            </Button>
        </header>
        {loading || !detail ? (
            <div className="flex flex-1 items-center justify-center">
                <Spinner size="md" tone="brand" label="正在读取" />
            </div>
        ) : (
            <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 py-3">
                {detail.relations.length > 0 && (
                    <section className="flex flex-col gap-1">
                        <h4 className="text-xs font-medium text-text-secondary">关系</h4>
                        {detail.relations.map((r) => {
                            const other = r.subject === id ? r.object : r.subject;
                            return (
                                <button
                                    key={r.hash}
                                    type="button"
                                    onClick={() => onGo(other)}
                                    className="group flex items-center gap-1.5 rounded-sm px-2 py-1.5 text-left text-xs hover:bg-inset"
                                >
                                    <span className={cn('truncate', r.subject === id ? 'text-text-secondary' : 'text-text')}>{r.subject}</span>
                                    <span className="shrink-0 text-brand">{r.predicate}</span>
                                    <span className={cn('min-w-0 flex-1 truncate', r.object === id ? 'text-text-secondary' : 'text-text')}>{r.object}</span>
                                    <ArrowRight size={12} className="shrink-0 text-text-disabled opacity-0 group-hover:opacity-100" />
                                </button>
                            );
                        })}
                    </section>
                )}
                {detail.paragraphs.length > 0 && (
                    <section className="flex flex-col gap-1.5">
                        <h4 className="text-xs font-medium text-text-secondary">出处</h4>
                        {detail.paragraphs.map((p) => (
                            <div key={p.hash} className="rounded-sm bg-field px-2.5 py-2">
                                <p className="line-clamp-3 text-xs leading-relaxed text-text">{p.preview}</p>
                                {p.source && <p className="mt-1 truncate text-2xs text-text-tertiary">{p.source}</p>}
                            </div>
                        ))}
                    </section>
                )}
            </div>
        )}
        {detail?.hash && (
            <footer className="border-t border-border-subtle px-3 py-2">
                <Button
                    size="sm"
                    variant="ghost"
                    className="text-danger hover:bg-danger-soft hover:text-danger"
                    disabled={busy}
                    onClick={() => onDelete(detail)}
                >
                    <Trash2 size={13} />
                    删掉这个点
                </Button>
            </footer>
        )}
    </aside>
);
