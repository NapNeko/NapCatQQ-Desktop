import { useEffect, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { Blocks, RefreshCw, Search, Settings } from 'lucide-react';
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
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { ConfigConflictDialog } from '../ConfigConflictDialog';
import { useKarinPlugins } from '../../../../hooks/apps/useKarinPlugins';
import { cn } from '../../../../shared/utils/cn';
import styles from './karinPluginsGrid.module.css';
import { KarinPluginConfigDialog } from './KarinPluginConfigDialog';
import { pluginCatalogErrorCopy, type VisiblePlugin } from './karinPluginsModel';
import type { AppInstance } from '../../../../core/ipc/types';

const TOOLBAR_SLOT_ID = 'karin-plugins-toolbar-slot';

const KIND_ITEMS = [
    { value: 'all', label: '全部类型' },
    { value: 'npm', label: 'npm' },
    { value: 'git', label: 'git' },
    { value: 'app', label: 'app' },
] as const;

const KIND_LABEL: Record<VisiblePlugin['kind'], string> = {
    npm: 'npm',
    git: 'git',
    app: 'app',
};

function PluginsToolbarPortal({ children }: { children: ReactNode }) {
    const [dock, setDock] = useState<HTMLElement | null>(() =>
        document.getElementById(TOOLBAR_SLOT_ID),
    );
    useEffect(() => {
        setDock(document.getElementById(TOOLBAR_SLOT_ID));
    }, []);
    if (!dock) return null;
    return createPortal(children, dock);
}

export const KarinPluginsTab: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const p = useKarinPlugins(instance);
    const [uninstall, setUninstall] = useState<string | null>(null);
    const [configName, setConfigName] = useState<string | null>(null);
    const err = p.error ? pluginCatalogErrorCopy(p.error) : null;
    const count = p.loading && p.rows.length === 0 ? '…' : String(p.rows.length);

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
                            aria-label="搜索插件"
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
                        items={[...KIND_ITEMS]}
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

            {err ? (
                <PagePlaceholder className="gap-3 py-16">
                    <p className="font-display text-md font-semibold text-text">{err.title}</p>
                    {err.detail && <p className="max-w-md text-sm text-text-secondary">{err.detail}</p>}
                    <Button size="sm" variant="primary" onClick={() => void p.reload()}>
                        重试
                    </Button>
                </PagePlaceholder>
            ) : p.loading && p.rows.length === 0 ? (
                <PagePlaceholder className="gap-3 py-16">
                    <Spinner size="lg" tone="brand" label="读取插件" />
                    <p className="text-sm text-text-tertiary">正在读取插件…</p>
                </PagePlaceholder>
            ) : p.rows.length === 0 ? (
                <PagePlaceholder className="gap-2 py-16">
                    <ActionMotionIcon icon={Blocks} size={28} className="text-text-tertiary" />
                    <p className="text-sm text-text-secondary">没有匹配的插件</p>
                </PagePlaceholder>
            ) : (
                <div className="min-h-0 flex-1 overflow-y-auto px-0.5 pb-4">
                    <div className={styles.grid}>
                        {p.rows.map((row) => (
                            <PluginCard
                                key={row.name}
                                row={row}
                                busy={p.busyNames.has(row.name)}
                                onInstall={() => void p.runOp(row.name, 'install')}
                                onUpdate={() => void p.runOp(row.name, 'update')}
                                onUninstall={() => setUninstall(row.name)}
                                onToggle={() => void p.applyEnabled(row.name, !row.enabled)}
                                onConfig={() => setConfigName(row.name)}
                            />
                        ))}
                    </div>
                </div>
            )}

            <Dialog open={uninstall !== null} onOpenChange={(o) => !o && setUninstall(null)}>
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>卸载插件？</DialogTitle>
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

            <KarinPluginConfigDialog
                instanceId={instance.id}
                pluginName={configName}
                onClose={() => setConfigName(null)}
            />

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

const PluginCard: React.FC<{
    row: VisiblePlugin;
    busy: boolean;
    onInstall: () => void;
    onUpdate: () => void;
    onUninstall: () => void;
    onToggle: () => void;
    onConfig: () => void;
}> = ({ row, busy, onInstall, onUpdate, onUninstall, onToggle, onConfig }) => {
    const accent = row.installed && row.enabled ? 'brand' : 'none';
    const meta = [row.authorName || null, row.timeLabel].filter(Boolean).join(' · ');

    return (
        <article
            className={cn(
                'relative isolate flex h-full min-h-[148px] min-w-0 flex-col overflow-hidden',
                'rounded-md border border-border-subtle bg-surface shadow-card',
                'transition-[box-shadow,border-color] duration-200 hover:border-border hover:shadow-popover',
                accent === 'brand' && 'ring-1 ring-inset ring-brand/25',
            )}
        >
            {accent === 'brand' && (
                <span aria-hidden className="absolute inset-y-0 left-0 w-0.5 bg-brand" />
            )}

            <div className="flex min-h-0 flex-1 flex-col justify-between px-4 pb-2.5 pt-3">
                <div className="min-w-0">
                    <h3 className="truncate font-display text-base font-semibold leading-snug text-text" title={row.name}>
                        {row.name}
                    </h3>
                    {meta ? (
                        <p className="mt-0.5 truncate text-2xs text-text-tertiary">{meta}</p>
                    ) : null}
                </div>
                {row.description ? (
                    <p className="mt-2 line-clamp-2 text-xs leading-snug text-text-secondary">{row.description}</p>
                ) : (
                    <span />
                )}
            </div>

            <footer className="flex h-11 shrink-0 items-center gap-2 border-t border-border-subtle bg-inset/35 px-3.5">
                <Badge tone="neutral" appearance="soft">
                    {KIND_LABEL[row.kind]}
                </Badge>
                {row.installed ? (
                    <Badge tone={row.enabled ? 'brand' : 'neutral'} appearance="soft">
                        {row.enabled ? '已装' : '已禁用'}
                    </Badge>
                ) : null}
                <div className="flex min-w-0 flex-1 items-center justify-end gap-1">
                    {busy && <Spinner size="sm" className="mr-0.5" />}
                    {row.installed ? (
                        <Popover>
                            <PopoverTrigger asChild>
                                <button
                                    type="button"
                                    disabled={busy}
                                    aria-label="插件操作"
                                    className={cn(
                                        'inline-flex h-8 w-8 items-center justify-center rounded-xs text-text-secondary',
                                        'transition-[color,background-color] duration-150 hover:bg-inset hover:text-text',
                                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                                        'disabled:cursor-not-allowed disabled:opacity-40',
                                    )}
                                >
                                    <Settings size={15} strokeWidth={2.2} />
                                </button>
                            </PopoverTrigger>
                            <PopoverContent align="end" sideOffset={6} className="w-36 p-1">
                                <PopoverClose asChild>
                                    <button type="button" className={menuItem} disabled={busy} onClick={onToggle}>
                                        {row.enabled ? '禁用' : '启用'}
                                    </button>
                                </PopoverClose>
                                <PopoverClose asChild>
                                    <button type="button" className={menuItem} disabled={busy} onClick={onUpdate}>
                                        更新
                                    </button>
                                </PopoverClose>
                                <PopoverClose asChild>
                                    <button type="button" className={menuItem} disabled={busy} onClick={onUninstall}>
                                        卸载
                                    </button>
                                </PopoverClose>
                                <PopoverClose asChild>
                                    <button type="button" className={menuItem} onClick={onConfig}>
                                        配置
                                    </button>
                                </PopoverClose>
                            </PopoverContent>
                        </Popover>
                    ) : (
                        <Button size="sm" variant="primary" disabled={busy} onClick={onInstall}>
                            {busy ? '安装中' : '安装'}
                        </Button>
                    )}
                </div>
            </footer>
        </article>
    );
};

const menuItem = cn(
    'flex w-full items-center rounded-sm px-2 py-1.5 text-left text-[13px] text-text hover:bg-inset',
    'disabled:pointer-events-none disabled:opacity-50',
);
