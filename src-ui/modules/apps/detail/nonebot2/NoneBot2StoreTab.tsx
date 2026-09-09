import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { Blocks, ChevronLeft, ChevronRight, ExternalLink, RefreshCw, Search, Settings } from 'lucide-react';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    PagePlaceholder,
    Popover,
    PopoverClose,
    PopoverContent,
    PopoverTrigger,
    Select,
    Spinner,
} from '../../../../shared/ui';
import { ActionMotionIcon, ListItem } from '../../../../shared/ui/motion';
import { ConfigConflictDialog } from '../ConfigConflictDialog';
import { KarinPluginConfigDialog } from '../karin/KarinPluginConfigDialog';
import { PaneLoading } from '../PaneStatus';
import { useNoneBot2Store } from '../../../../hooks/apps/useNoneBot2Store';
import { openExternalUrl } from '../../../../core/ipc/transport';
import { cn } from '../../../../shared/utils/cn';
import styles from './nonebot2StoreGrid.module.css';
import {
    paginateStore,
    storeGridFit,
    storePageItems,
    type StoreGridFit,
    type VisibleStoreItem,
} from './nonebot2StoreModel';
import type { AppInstance, AppStoreResource } from '../../../../core/ipc/types';

const TOOLBAR_SLOT_ID = 'app-store-toolbar-slot';

const FILTER_ITEMS = [
    { value: 'all', label: '全部' },
    { value: 'official', label: '官方' },
    { value: 'installed', label: '已装' },
] as const;

function PluginsToolbarPortal({ children }: { children: ReactNode }) {
    const [dock, setDock] = useState<HTMLElement | null>(() => document.getElementById(TOOLBAR_SLOT_ID));
    useEffect(() => {
        setDock(document.getElementById(TOOLBAR_SLOT_ID));
    }, []);
    if (!dock) return null;
    return createPortal(children, dock);
}

export const NoneBot2StoreTab: React.FC<{
    instance: AppInstance;
    resource: AppStoreResource;
}> = ({ instance, resource }) => {
    const p = useNoneBot2Store(instance, resource);
    const [uninstall, setUninstall] = useState<string | null>(null);
    const [configName, setConfigName] = useState<string | null>(null);
    const [page, setPage] = useState(0);
    const [fit, setFit] = useState<StoreGridFit>(() => storeGridFit(0, 0));
    const gridHostRef = useRef<HTMLDivElement>(null);
    const pageSizeRef = useRef(fit.pageSize);
    const kindLabel = resource === 'adapter' ? '适配器' : '插件';
    const count = p.loading && p.rows.length === 0 ? '…' : String(p.rows.length);
    const totalPages = Math.max(1, Math.ceil(p.rows.length / fit.pageSize));
    const safePage = Math.min(page, totalPages - 1);
    const visible = useMemo(
        () => paginateStore(p.rows, safePage, fit.pageSize),
        [p.rows, safePage, fit.pageSize],
    );
    const pages = useMemo(() => storePageItems(safePage, totalPages), [safePage, totalPages]);
    const showGrid = !(p.loading && p.rows.length === 0) && p.rows.length > 0;

    useEffect(() => {
        setPage(0);
    }, [instance.id, resource, p.query, p.kindFilter]);

    useEffect(() => {
        const prev = pageSizeRef.current;
        if (prev === fit.pageSize) return;
        setPage((current) => Math.floor((current * prev) / fit.pageSize));
        pageSizeRef.current = fit.pageSize;
    }, [fit.pageSize]);

    useLayoutEffect(() => {
        if (!showGrid) return;
        const el = gridHostRef.current;
        if (!el) return;
        const apply = () => {
            const next = storeGridFit(el.clientWidth, el.clientHeight);
            setFit((prev) => (prev.cols === next.cols && prev.rows === next.rows ? prev : next));
        };
        apply();
        const ro = new ResizeObserver(apply);
        ro.observe(el);
        return () => ro.disconnect();
    }, [showGrid]);

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <PluginsToolbarPortal>
                <div className="flex min-w-0 items-center gap-1.5 pr-1">
                    <div className="relative min-w-0">
                        <Search
                            size={13}
                            className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-text-tertiary"
                        />
                        <input
                            type="search"
                            aria-label={`搜索${kindLabel}`}
                            placeholder="搜索"
                            value={p.query}
                            onChange={(e) => p.setQuery(e.target.value)}
                            className={cn(
                                'h-7 w-36 rounded-sm border border-transparent bg-inset/70 pl-7 pr-2',
                                'text-[12px] text-text outline-none transition-colors',
                                'placeholder:text-text-tertiary',
                                'hover:border-border-subtle hover:bg-inset',
                                'focus:border-brand focus:bg-field focus:ring-2 focus:ring-brand focus:ring-inset',
                                'sm:w-48',
                            )}
                        />
                    </div>
                    <Select
                        items={[...FILTER_ITEMS]}
                        value={p.kindFilter}
                        onValueChange={p.setKindFilter}
                        className="w-[6.75rem] shrink-0 [&_button]:h-7 [&_button]:min-h-7 [&_button]:px-2 [&_button]:py-0 [&_button]:text-[12px]"
                    />
                    <span className="hidden min-w-[1.25rem] text-right text-2xs tabular-nums text-text-tertiary sm:inline">
                        {count}
                    </span>
                    <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 shrink-0"
                        aria-label="刷新"
                        onClick={() => void p.reload()}
                    >
                        <ActionMotionIcon icon={RefreshCw} size={13} motion={p.loading ? 'spin' : 'none'} />
                    </Button>
                </div>
            </PluginsToolbarPortal>

            {p.loading && p.rows.length === 0 ? (
                <PaneLoading text={`正在读取${kindLabel}…`} />
            ) : p.rows.length === 0 ? (
                <PagePlaceholder className="gap-2 py-16">
                    <ActionMotionIcon icon={Blocks} size={28} className="text-text-tertiary" />
                    <p className="text-sm text-text-secondary">
                        {p.error ? '目录未加载' : `没有匹配的${kindLabel}`}
                    </p>
                </PagePlaceholder>
            ) : (
                <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                    <div ref={gridHostRef} className="min-h-0 flex-1 overflow-hidden px-0.5 pb-2">
                        <div
                            className={styles.grid}
                            style={
                                {
                                    gridTemplateColumns: `repeat(${fit.cols}, minmax(0, 1fr))`,
                                    gridTemplateRows: `repeat(${fit.rows}, minmax(0, 1fr))`,
                                } as CSSProperties
                            }
                        >
                            {visible.map((row) => (
                                <ListItem key={row.id} hoverable className="flex h-full min-h-0 min-w-0 w-full">
                                    <StoreCard
                                        row={row}
                                        resource={resource}
                                        busy={p.busyNames.has(row.id) || p.busyNames.has(row.name)}
                                        onInstall={() => void p.runOp(row.id, 'install')}
                                        onUpdate={() => void p.runOp(row.id, 'update')}
                                        onUninstall={() => setUninstall(row.id)}
                                        onToggle={() => void p.applyEnabled(row.id, !row.enabled)}
                                        onConfig={resource === 'plugin' ? () => setConfigName(row.id) : undefined}
                                    />
                                </ListItem>
                            ))}
                        </div>
                    </div>
                    <nav
                        aria-label="分页"
                        className="flex h-11 shrink-0 items-center justify-center gap-0.5 border-t border-border-subtle bg-canvas px-2"
                    >
                        <button
                            type="button"
                            aria-label="上一页"
                            disabled={safePage <= 0}
                            onClick={() => setPage((n) => Math.max(0, n - 1))}
                            className={pagerBtn}
                        >
                            <ChevronLeft size={14} strokeWidth={2.2} />
                        </button>
                        {pages.map((item, i) =>
                            item === 'gap' ? (
                                <span key={`gap-${i}`} className="w-6 text-center text-2xs text-text-tertiary">
                                    …
                                </span>
                            ) : (
                                <button
                                    key={item}
                                    type="button"
                                    aria-current={item === safePage ? 'page' : undefined}
                                    onClick={() => setPage(item)}
                                    className={cn(pagerBtn, item === safePage && 'bg-inset font-medium text-text')}
                                >
                                    {item + 1}
                                </button>
                            ),
                        )}
                        <button
                            type="button"
                            aria-label="下一页"
                            disabled={safePage >= totalPages - 1}
                            onClick={() => setPage((n) => Math.min(totalPages - 1, n + 1))}
                            className={pagerBtn}
                        >
                            <ChevronRight size={14} strokeWidth={2.2} />
                        </button>
                    </nav>
                </div>
            )}

            <Dialog open={uninstall !== null} onOpenChange={(o) => !o && setUninstall(null)}>
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>卸载{kindLabel}？</DialogTitle>
                        <DialogDescription>此操作不可撤销。</DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button variant="ghost" size="sm" onClick={() => setUninstall(null)}>
                            取消
                        </Button>
                        <Button
                            variant="danger"
                            size="sm"
                            onClick={() => {
                                if (uninstall) void p.runOp(uninstall, 'uninstall');
                                setUninstall(null);
                            }}
                        >
                            卸载
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {resource === 'plugin' && (
                <KarinPluginConfigDialog
                    instanceId={instance.id}
                    pluginName={configName}
                    onClose={() => setConfigName(null)}
                />
            )}

            <ConfigConflictDialog
                open={p.conflict !== null}
                busy={p.conflictBusy}
                onCancel={p.dismissConflict}
                onReload={p.reloadConflict}
                onOverwrite={p.overwriteConflict}
            />
        </div>
    );
};

const StoreCard: React.FC<{
    row: VisibleStoreItem;
    resource: AppStoreResource;
    busy: boolean;
    onInstall: () => void;
    onUpdate: () => void;
    onUninstall: () => void;
    onToggle: () => void;
    onConfig?: () => void;
}> = ({ row, resource, busy, onInstall, onUpdate, onUninstall, onToggle, onConfig }) => {
    const kindLabel = resource === 'adapter' ? '适配器' : '插件';
    const meta = [row.authorName || null, row.timeLabel].filter(Boolean).join(' · ');
    const home = row.homepage.trim();

    return (
        <article
            className={cn(
                'relative flex h-full min-h-0 w-full min-w-0 flex-col gap-2 overflow-hidden rounded-md border border-border-subtle bg-surface px-4 py-3.5',
                'shadow-card transition-[border-color,box-shadow] duration-200 hover:border-border hover:shadow-popover',
                row.installed && row.enabled && 'ring-1 ring-inset ring-brand/20',
            )}
        >
            <div className="flex min-w-0 items-start gap-3">
                <div className="min-w-0 flex-1">
                    <div className="flex min-w-0 items-center gap-1.5">
                        <h3 className="min-w-0 truncate font-display text-[15px] font-semibold leading-6 text-text" title={row.name}>
                            {row.name}
                        </h3>
                        {home ? (
                            <button
                                type="button"
                                aria-label="打开主页"
                                className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-xs text-text-tertiary hover:bg-inset hover:text-text"
                                onClick={() => void openExternalUrl(home)}
                            >
                                <ExternalLink size={13} strokeWidth={2.2} />
                            </button>
                        ) : null}
                    </div>
                    <p className="mt-0.5 truncate text-2xs leading-4 text-text-tertiary">{meta || '\u00a0'}</p>
                </div>
                <div className="flex shrink-0 items-center gap-1 pt-0.5">
                    {busy && <Spinner size="sm" />}
                    {row.installed ? (
                        <Popover>
                            <PopoverTrigger asChild>
                                <button
                                    type="button"
                                    disabled={busy}
                                    aria-label={`${kindLabel}操作`}
                                    className={iconBtn}
                                >
                                    <Settings size={15} strokeWidth={2.2} />
                                </button>
                            </PopoverTrigger>
                            <PopoverContent align="end" sideOffset={6} className="w-36 p-1">
                                <PopoverClose asChild>
                                    <button
                                        type="button"
                                        className={menuItem}
                                        disabled={busy || row.locked}
                                        onClick={onToggle}
                                    >
                                        {row.enabled ? '禁用' : '启用'}
                                    </button>
                                </PopoverClose>
                                <PopoverClose asChild>
                                    <button type="button" className={menuItem} disabled={busy} onClick={onUpdate}>
                                        更新
                                    </button>
                                </PopoverClose>
                                <PopoverClose asChild>
                                    <button
                                        type="button"
                                        className={menuItem}
                                        disabled={busy || row.locked}
                                        onClick={onUninstall}
                                    >
                                        卸载
                                    </button>
                                </PopoverClose>
                                {onConfig && (
                                    <PopoverClose asChild>
                                        <button type="button" className={menuItem} onClick={onConfig}>
                                            配置
                                        </button>
                                    </PopoverClose>
                                )}
                            </PopoverContent>
                        </Popover>
                    ) : (
                        <button type="button" disabled={busy} onClick={onInstall} className={installBtn}>
                            安装
                        </button>
                    )}
                </div>
            </div>

            <p className="mt-auto line-clamp-2 min-h-10 text-[13px] leading-5 text-text-secondary">
                {row.description || '\u00a0'}
            </p>

            <div className="flex h-5 min-w-0 items-center gap-1.5">
                {row.official ? (
                    <Badge tone="brand" appearance="soft">
                        官方
                    </Badge>
                ) : null}
                {row.installed ? (
                    <Badge tone={row.enabled ? 'brand' : 'neutral'} appearance="soft">
                        {row.enabled ? '已启用' : '已禁用'}
                    </Badge>
                ) : null}
                {row.locked ? <span className="text-2xs text-text-tertiary">已对接不能关</span> : null}
            </div>
        </article>
    );
};

const pagerBtn = cn(
    'inline-flex h-7 min-w-7 items-center justify-center rounded-xs px-1.5 text-[12px] tabular-nums text-text-secondary',
    'hover:bg-inset hover:text-text',
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
    'disabled:pointer-events-none disabled:opacity-30',
);

const iconBtn = cn(
    'inline-flex h-7 w-7 items-center justify-center rounded-xs text-text-secondary',
    'hover:bg-inset hover:text-text',
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
    'disabled:cursor-not-allowed disabled:opacity-40',
);

const installBtn = cn(
    'h-7 rounded-xs px-2 text-[12px] font-medium text-brand',
    'hover:bg-brand-soft',
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
    'disabled:pointer-events-none disabled:opacity-40',
);

const menuItem = cn(
    'flex w-full items-center rounded-sm px-2 py-1.5 text-left text-[13px] text-text hover:bg-inset',
    'disabled:pointer-events-none disabled:opacity-50',
);
