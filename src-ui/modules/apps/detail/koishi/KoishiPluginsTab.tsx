// 插件页：左边是 koishi.yml 的插件树（分组可折叠、每项一个开关），右边是选中项的详情。
// 选中项按键（名字:标识）记，删改挪之后下标变了也还能找回来。
// 「添加」从已装的包里挑（没进树的排前面），装新包去插件市场。

import { useEffect, useMemo, useRef, useState } from 'react';
import { ChevronRight, Folder, FolderOpen, FolderPlus, Lock, Plus, Puzzle, Search, Store } from 'lucide-react';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import {
    KOISHI_CORE_PLUGINS,
    appendTo,
    effective,
    isGroup,
    isLinkNode,
    newGroup,
    newPlugin,
    nodeAt,
    nodeKey,
    nodeLabel,
    nodePathOfIssue,
    replaceAt,
    walk,
    type NodePath,
} from '../../../../core/domain/apps/koishiConfig';
import { useKoishiPackages } from '../../../../hooks/apps/useKoishiRuntime';
import { KoishiNodeDetail } from './koishiPluginDetail';
import type { AppInstance, KoishiInstanceConfig, KoishiPluginNode } from '../../../../core/ipc/types';

function findPath(nodes: readonly KoishiPluginNode[], key: string, base: number[] = []): number[] | null {
    for (let i = 0; i < nodes.length; i += 1) {
        if (nodeKey(nodes[i]) === key) return [...base, i];
        const hit = findPath(nodes[i].children, key, [...base, i]);
        if (hit) return hit;
    }
    return null;
}

/** 选中节点真正生效没有：沿路径每一层都开着 */
function liveAt(cfg: KoishiInstanceConfig, path: NodePath): boolean {
    let list = cfg.plugins;
    for (const i of path) {
        const n = list[i];
        if (!n?.enabled) return false;
        list = n.children;
    }
    return true;
}

function matches(node: KoishiPluginNode, q: string): boolean {
    if (!q) return true;
    const hay = `${node.name} ${node.ident} ${nodeLabel(node)}`.toLowerCase();
    return hay.includes(q) || node.children.some((c) => matches(c, q));
}

/** 搜索框：TextField 的 className 落在外层，放不进图标，这里单独做一个 */
function SearchBox({ value, onChange }: { value: string; onChange: (v: string) => void }) {
    return (
        <label className="flex h-9 items-center gap-2 rounded-md border border-border-subtle bg-inset/60 px-2.5 text-text-tertiary focus-within:border-brand/50 focus-within:text-text-secondary">
            <Search size={14} className="shrink-0" />
            <input
                value={value}
                placeholder="找插件"
                aria-label="找插件"
                className="min-w-0 flex-1 bg-transparent text-[13px] text-text outline-none placeholder:text-text-disabled"
                onChange={(e) => onChange(e.target.value)}
            />
        </label>
    );
}

/**
 * 树里的开关：整列大开关太抢眼，开没开左边的圆点和字色已经说了。
 * 平时藏着，悬停、选中或键盘聚焦时才出来，尺寸也压到行高里
 */
function MiniToggle({
    label,
    checked,
    disabled,
    visible,
    onChange,
}: {
    label: string;
    checked: boolean;
    disabled?: boolean;
    visible: boolean;
    onChange: (next: boolean) => void;
}) {
    return (
        <button
            type="button"
            role="switch"
            aria-checked={checked}
            aria-label={label}
            title={label}
            disabled={disabled}
            onClick={(e) => {
                e.stopPropagation();
                onChange(!checked);
            }}
            className={cn(
                'relative h-4 w-7 shrink-0 rounded-full transition-[background-color,opacity] duration-200',
                'focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40',
                'disabled:cursor-not-allowed disabled:opacity-40',
                checked ? 'bg-brand/85' : 'bg-border',
                visible ? 'opacity-100' : 'opacity-0 group-hover:opacity-100',
            )}
        >
            <span
                className={cn(
                    'absolute top-0.5 h-3 w-3 rounded-full bg-white shadow-sm transition-[left] duration-200',
                    checked ? 'left-[14px]' : 'left-0.5',
                )}
            />
        </button>
    );
}

interface RowsProps {
    nodes: readonly KoishiPluginNode[];
    base: number[];
    depth: number;
    selected: string | null;
    onSelect: (key: string) => void;
    onToggle: (path: NodePath, enabled: boolean) => void;
    errorKeys: ReadonlySet<string>;
    query: string;
    disabled?: boolean;
    parentOn: boolean;
}

function TreeRows(props: RowsProps) {
    const { nodes, base, depth, selected, onSelect, onToggle, errorKeys, query, disabled, parentOn } = props;
    const [folded, setFolded] = useState<Record<string, boolean>>({});
    return (
        <>
            {nodes.map((n, i) => {
                if (!matches(n, query)) return null;
                const key = nodeKey(n);
                const path = [...base, i];
                const group = isGroup(n);
                const open = query ? true : !folded[key];
                const locked = KOISHI_CORE_PLUGINS.has(n.name) && n.enabled;
                const live = parentOn && n.enabled;
                const isSel = selected === key;
                const count = group ? walk(n.children).filter((c) => !isGroup(c)).length : 0;
                const GroupIcon = open ? FolderOpen : Folder;
                return (
                    <li key={key} className="flex flex-col">
                        <div
                            data-node-key={key}
                            className={cn(
                                'group relative flex h-8 items-center gap-1.5 rounded-md pr-1.5 text-[13px] transition-colors',
                                isSel ? 'bg-brand-soft' : 'hover:bg-inset',
                            )}
                            style={{ paddingLeft: 6 + depth * 16 }}
                        >
                            {isSel && <span className="absolute inset-y-1.5 left-0 w-0.5 rounded-full bg-brand" aria-hidden />}
                            {group ? (
                                <button
                                    type="button"
                                    aria-label={open ? '收起' : '展开'}
                                    className="flex h-5 w-5 shrink-0 items-center justify-center rounded text-text-tertiary hover:text-text"
                                    onClick={() => setFolded((f) => ({ ...f, [key]: open }))}
                                >
                                    <ChevronRight size={13} className={cn('transition-transform duration-200', open && 'rotate-90')} />
                                </button>
                            ) : (
                                <span className="flex h-5 w-5 shrink-0 items-center justify-center" aria-hidden>
                                    <span className={cn('h-1.5 w-1.5 rounded-full', live ? 'bg-success' : 'bg-text-disabled/60')} />
                                </span>
                            )}
                            <button
                                type="button"
                                className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
                                onClick={() => onSelect(key)}
                            >
                                {group && (
                                    <GroupIcon size={14} className={cn('shrink-0', live ? 'text-brand/80' : 'text-text-disabled')} />
                                )}
                                <span
                                    className={cn(
                                        'truncate',
                                        group && 'font-medium',
                                        live ? 'text-text' : 'text-text-tertiary',
                                        errorKeys.has(key) && 'text-danger',
                                    )}
                                >
                                    {nodeLabel(n)}
                                </span>
                                {group && count > 0 && (
                                    <span className="shrink-0 rounded-pill bg-inset px-1.5 text-2xs tabular-nums text-text-tertiary">
                                        {count}
                                    </span>
                                )}
                                {isLinkNode(n) && <Badge tone="brand">对接</Badge>}
                                {typeof n.meta.$if === 'string' && (
                                    <span className="shrink-0 text-2xs text-text-disabled" title={String(n.meta.$if)}>
                                        按条件
                                    </span>
                                )}
                            </button>
                            {locked ? (
                                <span
                                    title="Koishi 和桌面端要用它，不能停"
                                    className="flex h-5 w-7 shrink-0 items-center justify-center text-text-disabled opacity-0 transition-opacity group-hover:opacity-100"
                                >
                                    <Lock size={11} />
                                </span>
                            ) : (
                                <MiniToggle
                                    label={n.enabled ? `停用 ${nodeLabel(n)}` : `启用 ${nodeLabel(n)}`}
                                    checked={n.enabled}
                                    disabled={disabled}
                                    visible={isSel}
                                    onChange={(v) => onToggle(path, v)}
                                />
                            )}
                        </div>
                        {group && open && n.children.length > 0 && (
                            <ul className="flex flex-col">
                                <TreeRows {...props} nodes={n.children} base={path} depth={depth + 1} parentOn={live} />
                            </ul>
                        )}
                    </li>
                );
            })}
        </>
    );
}

export const KoishiPluginsTab: React.FC<{
    instance: AppInstance;
    config: KoishiInstanceConfig;
    onChange: (next: KoishiInstanceConfig) => void;
    errors: Record<string, string>;
    disabled?: boolean;
    onGoMarket: () => void;
    /** 从插件市场的齿轮跳过来：选中这个插件（seq 变了才算一次新的跳转） */
    focus?: { name: string; seq: number } | null;
}> = ({ instance, config, onChange, errors, disabled, onGoMarket, focus }) => {
    const [selected, setSelected] = useState<string | null>(null);
    const [query, setQuery] = useState('');
    const [adding, setAdding] = useState(false);
    const treeRef = useRef<HTMLUListElement>(null);
    useEffect(() => {
        if (!focus) return;
        const hit = walk(config.plugins).find((n) => n.name === focus.name);
        if (hit) setSelected(nodeKey(hit));
        // 只在跳转那一下选；之后用户点别的不再被拉回来
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [focus?.seq]);

    // 新加的、从市场跳过来的项可能在树的下半截，选中后滚到看得见
    useEffect(() => {
        if (!selected) return;
        const row = treeRef.current?.querySelector(`[data-node-key="${CSS.escape(selected)}"]`);
        row?.scrollIntoView({ block: 'nearest' });
    }, [selected]);

    const path = selected ? findPath(config.plugins, selected) : null;
    const node = path ? nodeAt(config, path) : undefined;
    // 新建放到选中的分组里；选中的是插件就放它旁边
    const targetGroup = path && node ? (isGroup(node) ? path : path.slice(0, -1)) : [];
    const q = query.trim().toLowerCase();

    const errorKeys = useMemo(() => {
        const out = new Set<string>();
        for (const p of Object.keys(errors)) {
            const np = nodePathOfIssue(p);
            const n = np ? nodeAt(config, np) : undefined;
            if (n) out.add(nodeKey(n));
        }
        return out;
    }, [errors, config]);

    const total = walk(config.plugins).filter((n) => !isGroup(n)).length;
    const on = effective(config.plugins).filter((n) => !isGroup(n)).length;

    return (
        <div className="flex min-h-0 flex-1 gap-5 pb-3">
            <aside className="flex w-[280px] shrink-0 flex-col overflow-hidden rounded-lg border border-border-subtle bg-surface shadow-card">
                <div className="flex flex-col gap-2.5 border-b border-border-subtle/70 p-3">
                    <SearchBox value={query} onChange={setQuery} />
                    <div className="flex items-center justify-between gap-2">
                        <span className="text-2xs text-text-tertiary">
                            <span className="font-medium tabular-nums text-text-secondary">{on}</span> / {total} 个在用
                        </span>
                        <div className="flex items-center gap-1">
                            <Button size="sm" variant="ghost" disabled={disabled} onClick={() => setAdding(true)}>
                                <Plus size={13} />
                                添加
                            </Button>
                            <Button
                                size="sm"
                                variant="ghost"
                                disabled={disabled}
                                onClick={() => {
                                    const g = newGroup(config, '新分组');
                                    onChange(appendTo(config, targetGroup, g));
                                    setSelected(nodeKey(g));
                                    // 新建的多半不匹配当前搜索，清掉免得选中了却在树里看不见
                                    setQuery('');
                                }}
                            >
                                <FolderPlus size={13} />
                                分组
                            </Button>
                        </div>
                    </div>
                </div>
                <ul ref={treeRef} className="min-h-0 flex-1 overflow-y-auto p-1.5">
                    <TreeRows
                        nodes={config.plugins}
                        base={[]}
                        depth={0}
                        selected={selected}
                        onSelect={setSelected}
                        onToggle={(p, enabled) => onChange(replaceAt(config, p, (n) => ({ ...n, enabled })))}
                        errorKeys={errorKeys}
                        query={q}
                        disabled={disabled}
                        parentOn
                    />
                    {q && !config.plugins.some((n) => matches(n, q)) && (
                        <li className="px-3 py-6 text-center text-xs text-text-tertiary">树里没有「{query.trim()}」</li>
                    )}
                </ul>
            </aside>
            <section className="min-w-0 flex-1 overflow-y-auto pb-6 pr-1">
                {node && path ? (
                    <KoishiNodeDetail
                        instanceId={instance.id}
                        config={config}
                        path={path}
                        node={node}
                        live={liveAt(config, path)}
                        onChange={onChange}
                        onSelect={setSelected}
                        disabled={disabled}
                    />
                ) : (
                    <EmptyDetail onGoMarket={onGoMarket} onAdd={() => setAdding(true)} />
                )}
            </section>
            <AddPluginDialog
                open={adding}
                instanceId={instance.id}
                config={config}
                onClose={() => setAdding(false)}
                onGoMarket={() => {
                    setAdding(false);
                    onGoMarket();
                }}
                onAdd={(name) => {
                    const n = newPlugin(config, name, false);
                    onChange(appendTo(config, targetGroup, n));
                    setSelected(nodeKey(n));
                    setQuery('');
                    setAdding(false);
                }}
            />
        </div>
    );
};

function EmptyDetail({ onGoMarket, onAdd }: { onGoMarket: () => void; onAdd: () => void }) {
    return (
        <div className="flex h-full min-h-[320px] flex-col items-center justify-center gap-4 rounded-lg border border-dashed border-border-subtle px-8 text-center">
            <span className="flex h-12 w-12 items-center justify-center rounded-lg bg-inset text-text-tertiary">
                <Puzzle size={22} />
            </span>
            <div className="flex flex-col gap-1.5">
                <p className="text-[14px] font-medium text-text">在左边选一个插件</p>
                <p className="max-w-sm text-xs leading-relaxed text-text-tertiary">
                    表单按插件自己声明的配置画；开关和配置改完点底部保存，Koishi 在跑的话当场生效
                </p>
            </div>
            <div className="flex items-center gap-2">
                <Button size="sm" variant="secondary" onClick={onAdd}>
                    <Plus size={13} />
                    添加已装的插件
                </Button>
                <Button size="sm" variant="ghost" onClick={onGoMarket}>
                    <Store size={13} />
                    去插件市场
                </Button>
            </div>
        </div>
    );
}

function AddPluginDialog({
    open,
    instanceId,
    config,
    onClose,
    onAdd,
    onGoMarket,
}: {
    open: boolean;
    instanceId: string;
    config: KoishiInstanceConfig;
    onClose: () => void;
    onAdd: (name: string) => void;
    onGoMarket: () => void;
}) {
    const packages = useKoishiPackages(instanceId, open);
    const [q, setQ] = useState('');
    const inTree = new Set(walk(config.plugins).map((n) => n.name));
    const rows = (packages.data ?? [])
        .filter((p) => !q || `${p.name} ${p.package} ${p.description}`.toLowerCase().includes(q.toLowerCase()))
        .sort((a, b) => Number(inTree.has(a.name)) - Number(inTree.has(b.name)) || a.name.localeCompare(b.name));
    return (
        <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
            <DialogContent size="lg">
                <DialogHeader>
                    <DialogTitle>添加插件</DialogTitle>
                    <DialogDescription>
                        从这个实例已经装好的包里挑，加进来先是停用的，配好再开；同一个插件可以加多份，各配各的
                    </DialogDescription>
                </DialogHeader>
                <SearchBox value={q} onChange={setQ} />
                <ul className="-mx-1 flex max-h-[46vh] flex-col overflow-y-auto">
                    {packages.isLoading && <li className="py-8 text-center text-sm text-text-tertiary">正在读取已装的包…</li>}
                    {packages.error && (
                        <li className="py-8 text-center text-sm text-danger">读取失败：{packages.error.message}</li>
                    )}
                    {rows.map((p) => (
                        <li key={p.package}>
                            <button
                                type="button"
                                className="group flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-left transition-colors hover:bg-inset"
                                onClick={() => onAdd(p.name)}
                            >
                                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-inset text-text-tertiary transition-colors group-hover:bg-brand-soft group-hover:text-brand">
                                    <Puzzle size={15} />
                                </span>
                                <div className="min-w-0 flex-1">
                                    <div className="flex items-center gap-2 text-[13px] font-medium text-text">
                                        {p.name}
                                        {inTree.has(p.name) && <Badge tone="neutral">已在树里</Badge>}
                                        {!p.version && <Badge tone="warning">没装上</Badge>}
                                    </div>
                                    <div className="mt-0.5 truncate text-2xs text-text-tertiary">
                                        {p.description || p.package}
                                        {p.version && <span className="ml-1.5 font-mono text-text-disabled">v{p.version}</span>}
                                    </div>
                                </div>
                                <Plus size={15} className="shrink-0 text-text-disabled transition-colors group-hover:text-brand" />
                            </button>
                        </li>
                    ))}
                </ul>
                <DialogFooter>
                    <Button variant="ghost" size="sm" onClick={onGoMarket}>
                        <Store size={13} />
                        去插件市场装新的
                    </Button>
                    <Button variant="secondary" size="sm" onClick={onClose}>
                        关闭
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
