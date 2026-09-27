import { useState } from 'react';
import { Blocks, Settings } from 'lucide-react';
import {
    Badge,
    Button,
    PagePlaceholder,
    Popover,
    PopoverClose,
    PopoverContent,
    PopoverTrigger,
    Spinner,
} from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { ConfigConflictDialog } from '../ConfigConflictDialog';
import { PaneLoading } from '../PaneStatus';
import { StoreToolbar, UninstallDialog } from '../storeToolbar';
import { useKarinPlugins } from '../../../../hooks/apps/useKarinPlugins';
import { cn } from '../../../../shared/utils/cn';
import styles from './karinPluginsGrid.module.css';
import { PluginConfigDialog } from '../PluginConfigDialog';
import { type VisiblePlugin } from '../../../../core/domain/apps/karinPlugins';
import type { AppInstance } from '../../../../core/ipc/types';

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

export const KarinPluginsTab: React.FC<{ instance: AppInstance }> = ({ instance }) => {
    const p = useKarinPlugins(instance);
    const [uninstall, setUninstall] = useState<string | null>(null);
    const [configName, setConfigName] = useState<string | null>(null);
    const count = p.loading && p.rows.length === 0 ? '…' : String(p.rows.length);

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <StoreToolbar
                noun="插件"
                query={p.query}
                onQueryChange={p.setQuery}
                filters={KIND_ITEMS}
                filter={p.kindFilter}
                onFilterChange={p.setKindFilter}
                count={count}
                loading={p.loading}
                onReload={() => void p.reload()}
            />

            {p.loading && p.rows.length === 0 ? (
                <PaneLoading text="正在读取插件…" />
            ) : p.rows.length === 0 ? (
                <PagePlaceholder className="gap-2 py-16">
                    <ActionMotionIcon icon={Blocks} size={28} className="text-text-tertiary" />
                    <p className="text-sm text-text-secondary">
                        {p.error ? '目录未加载' : '没有匹配的插件'}
                    </p>
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

            <UninstallDialog
                noun="插件"
                target={uninstall}
                onClose={() => setUninstall(null)}
                onConfirm={(name) => void p.runOp(name, 'uninstall')}
            />

            <PluginConfigDialog
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
