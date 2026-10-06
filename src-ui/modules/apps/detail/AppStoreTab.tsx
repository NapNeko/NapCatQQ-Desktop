import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { Blocks, ChevronLeft, ChevronRight, ExternalLink, Settings } from 'lucide-react';
import {
    Badge,
    PagePlaceholder,
    Popover,
    PopoverClose,
    PopoverContent,
    PopoverTrigger,
    Spinner,
    type SelectItem,
} from '../../../shared/ui';
import { ActionMotionIcon, ListItem } from '../../../shared/ui/motion';
import { ConfigConflictDialog } from './ConfigConflictDialog';
import { PluginConfigDialog } from './PluginConfigDialog';
import { PaneLoading } from './PaneStatus';
import { StoreToolbar, UninstallDialog } from './storeToolbar';
import { useAppStore } from '../../../hooks/apps/useAppStore';
import { useOpenExternal } from '../../../hooks/useOpenExternal';
import { cn } from '../../../shared/utils/cn';
import styles from './appStoreGrid.module.css';
import {
    paginateStore,
    storeGridFit,
    storePageItems,
    type StoreGridFit,
    type StoreKindFilter,
    type VisibleStoreItem,
} from '../../../core/domain/apps/appStore';
import type { AppInstance, AppStoreResource } from '../../../core/ipc/types';

function filterItems(
    officialLabel: string,
    categories: readonly string[],
): SelectItem<StoreKindFilter>[] {
    return [
        { value: 'all', label: '全部' },
        { value: 'official', label: officialLabel },
        ...categories.map((c) => ({ value: `tag:${c}` as const, label: c })),
        { value: 'installed', label: '已装' },
    ];
}

/** 应用端商店页：NoneBot2 的适配器、插件两页，AstrBot、麦麦、云崽的插件页 */
export const AppStoreTab: React.FC<{
    instance: AppInstance;
    resource: AppStoreResource;
    /** 目录里 is_official 那批叫什么；云崽的索引没有官方，只有推荐 */
    officialLabel?: string;
    /** 目录按分类打了标签时的筛选项（和条目 tags 对得上的那几个） */
    categories?: readonly string[];
    /** 哪些已装的能单独启停；不给就都能（云崽只有单 JS 能停，目录插件整个加载） */
    toggleable?: (id: string) => boolean;
    toggleBlockedReason?: string;
    /** 插件配置不在单独文件里的框架（Koishi 在插件树里）：齿轮跳过去，不开配置框 */
    onConfigure?: (id: string, name: string) => void;
}> = ({
    instance,
    resource,
    officialLabel = '官方',
    categories,
    toggleable,
    toggleBlockedReason,
    onConfigure,
}) => {
    const filters = useMemo(
        () => filterItems(officialLabel, categories ?? []),
        [officialLabel, categories],
    );
    const p = useAppStore(instance, resource);
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
            <StoreToolbar
                noun={kindLabel}
                query={p.query}
                onQueryChange={p.setQuery}
                filters={filters}
                filter={p.kindFilter}
                onFilterChange={p.setKindFilter}
                count={count}
                loading={p.loading}
                onReload={() => void p.reload()}
            />

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
                                <ListItem
                                    key={row.id}
                                    hoverable
                                    className="flex h-full min-h-0 min-w-0 w-full"
                                >
                                    <StoreCard
                                        row={row}
                                        resource={resource}
                                        officialLabel={officialLabel}
                                        showTags={!!categories?.length}
                                        toggleBlocked={
                                            toggleable && !toggleable(row.id)
                                                ? (toggleBlockedReason ?? '不能单独启停')
                                                : null
                                        }
                                        busy={p.busyNames.has(row.id) || p.busyNames.has(row.name)}
                                        onInstall={() => void p.runOp(row.id, 'install')}
                                        onUpdate={() => void p.runOp(row.id, 'update')}
                                        onUninstall={() => setUninstall(row.id)}
                                        onToggle={() => void p.applyEnabled(row.id, !row.enabled)}
                                        onConfig={
                                            resource !== 'plugin'
                                                ? undefined
                                                : onConfigure
                                                  ? () => onConfigure(row.id, row.name)
                                                  : () => setConfigName(row.id)
                                        }
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
                            title="上一页"
                            disabled={safePage <= 0}
                            onClick={() => setPage((n) => Math.max(0, n - 1))}
                            className={pagerBtn}
                        >
                            <ChevronLeft size={14} strokeWidth={2.2} />
                        </button>
                        {pages.map((item, i) =>
                            item === 'gap' ? (
                                <span
                                    key={`gap-${i}`}
                                    className="w-6 text-center text-2xs text-text-tertiary"
                                >
                                    …
                                </span>
                            ) : (
                                <button
                                    key={item}
                                    type="button"
                                    aria-current={item === safePage ? 'page' : undefined}
                                    onClick={() => setPage(item)}
                                    className={cn(
                                        pagerBtn,
                                        item === safePage && 'bg-inset font-medium text-text',
                                    )}
                                >
                                    {item + 1}
                                </button>
                            ),
                        )}
                        <button
                            type="button"
                            aria-label="下一页"
                            title="下一页"
                            disabled={safePage >= totalPages - 1}
                            onClick={() => setPage((n) => Math.min(totalPages - 1, n + 1))}
                            className={pagerBtn}
                        >
                            <ChevronRight size={14} strokeWidth={2.2} />
                        </button>
                    </nav>
                </div>
            )}

            <UninstallDialog
                noun={kindLabel}
                target={uninstall}
                onClose={() => setUninstall(null)}
                onConfirm={(id) => void p.runOp(id, 'uninstall')}
            />

            {resource === 'plugin' && !onConfigure && (
                <PluginConfigDialog
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
    officialLabel: string;
    showTags: boolean;
    /** 不能单独启停时的原因；null 就是能 */
    toggleBlocked: string | null;
    busy: boolean;
    onInstall: () => void;
    onUpdate: () => void;
    onUninstall: () => void;
    onToggle: () => void;
    onConfig?: () => void;
}> = ({
    row,
    resource,
    officialLabel,
    showTags,
    toggleBlocked,
    busy,
    onInstall,
    onUpdate,
    onUninstall,
    onToggle,
    onConfig,
}) => {
    // 「推荐」已经单独有徽章，分类徽章只放剩下的
    const tags = showTags ? row.tags.filter((t) => t !== officialLabel) : [];
    const kindLabel = resource === 'adapter' ? '适配器' : '插件';
    const meta = [row.authorName || null, row.timeLabel].filter(Boolean).join(' · ');
    const home = row.homepage.trim();
    const openExternal = useOpenExternal();

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
                        <h3
                            className="min-w-0 truncate font-display text-[15px] font-semibold leading-6 text-text"
                            title={row.name}
                        >
                            {row.name}
                        </h3>
                        {home ? (
                            <button
                                type="button"
                                aria-label="打开主页"
                                className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-xs text-text-tertiary hover:bg-inset hover:text-text"
                                onClick={() => openExternal(home)}
                            >
                                <ExternalLink size={13} strokeWidth={2.2} />
                            </button>
                        ) : null}
                    </div>
                    <p className="mt-0.5 truncate text-2xs leading-4 text-text-tertiary">
                        {meta || '\u00a0'}
                    </p>
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
                                {toggleBlocked === null ? (
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
                                ) : (
                                    <p className="px-2 py-1.5 text-2xs leading-4 text-text-tertiary">
                                        {toggleBlocked}
                                    </p>
                                )}
                                <PopoverClose asChild>
                                    <button
                                        type="button"
                                        className={menuItem}
                                        disabled={busy || row.managed}
                                        onClick={onUpdate}
                                    >
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
                                        <button
                                            type="button"
                                            className={menuItem}
                                            onClick={onConfig}
                                        >
                                            配置
                                        </button>
                                    </PopoverClose>
                                )}
                            </PopoverContent>
                        </Popover>
                    ) : row.installable ? (
                        <button
                            type="button"
                            disabled={busy}
                            onClick={onInstall}
                            className={installBtn}
                        >
                            安装
                        </button>
                    ) : (
                        <button
                            type="button"
                            disabled={!home}
                            title="目录里没有能直接装的地址，去主页看怎么装"
                            onClick={() => openExternal(home)}
                            className={installBtn}
                        >
                            手动装
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
                        {officialLabel}
                    </Badge>
                ) : null}
                {tags.map((t) => (
                    <Badge key={t} tone="neutral" appearance="outline">
                        {t}
                    </Badge>
                ))}
                {row.installed ? (
                    <Badge tone={row.enabled ? 'brand' : 'neutral'} appearance="soft">
                        {row.enabled ? '已启用' : '已禁用'}
                    </Badge>
                ) : null}
                {row.locked ? (
                    <span className="text-2xs text-text-tertiary">
                        {row.lockReason ?? '已对接不能关'}
                    </span>
                ) : null}
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
