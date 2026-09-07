// 应用端实例列表：卡式布局对齐 Bot / 组件页；点卡进详情，底栏做启停与对接。

import React, { useMemo, useRef, useState } from 'react';
import {
    Blocks,
    Boxes,
    Download,
    ExternalLink,
    Link2,
    Play,
    RefreshCw,
    ScrollText,
    Square,
    Trash2,
    Unlink,
} from 'lucide-react';
import { useGSAP } from '@gsap/react';
import { animateListChildrenEnter } from '../../../shared/ui/motion/listEnter';
import {
    Badge,
    Button,
    PagePlaceholder,
    Spinner,
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from '../../../shared/ui';
import {
    ActionMotionIcon,
    Counter,
    EMPHASIS_MOTION,
    ListItem,
    RESOURCE_MOTION,
    refreshMotion,
} from '../../../shared/ui/motion';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { useServerManager } from '../../../hooks/remote/useServerManager';
import { useAppFrameworks, useAppInstances } from '../../../hooks/apps/useAppInstances';
import { cn } from '../../../shared/utils/cn';
import { AppLinkDialog } from '../AppLinkDialog';
import { DeleteInstanceDialog } from '../DeleteInstanceDialog';
import { hostIdDisplayLabel } from '../hostLabel';
import { STATE_META, isInstalled } from '../instanceState';
import type { AppRoute } from '../../../shared/components/next/Sidebar';
import type { AppFrameworkManifest, AppInstance } from '../../../core/ipc/types';
import gridStyles from './appCardGrid.module.css';

export type DetailTabHint = 'config' | 'log';

export interface AppInstanceListPageProps {
    onNavigate?: (route: AppRoute) => void;
    onOpenInstance: (instanceId: string, tab?: DetailTabHint) => void;
}

export const AppInstanceListPage: React.FC<AppInstanceListPageProps> = ({ onNavigate, onOpenInstance }) => {
    const frameworks = useAppFrameworks();
    const apps = useAppInstances();
    const { servers } = useServerManager();

    const [linkInstanceId, setLinkInstanceId] = useState<string | null>(null);
    const [deleteInstance, setDeleteInstance] = useState<AppInstance | null>(null);

    const manifests = frameworks.data ?? [];
    const manifestById = useMemo(
        () => new Map(manifests.map((m) => [m.id, m] as const)),
        [manifests],
    );

    const refreshing = frameworks.isFetching || apps.isLoading;

    return (
        <TooltipProvider delayDuration={200}>
            <div className="flex min-h-0 flex-1 flex-col">
                <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
                    <div>
                        <p className="text-2xs uppercase tracking-widest text-text-tertiary">apps</p>
                        <h1 className="font-display text-xl font-semibold text-text">应用端</h1>
                        <p className="mt-1 text-sm text-text-secondary">管理应用实例的启停、对接与配置。</p>
                    </div>
                    <div className="flex items-center gap-3">
                        <div className="hidden items-baseline gap-1 text-xs text-text-tertiary tabular-nums sm:flex">
                            <span>共</span>
                            <Counter value={apps.instances.length} className="font-medium text-text-secondary" />
                            <span>个实例</span>
                        </div>
                        {onNavigate && (
                            <Button size="sm" variant="secondary" onClick={() => onNavigate('components')}>
                                <ActionMotionIcon icon={Boxes} size={14} />
                                去组件页安装
                            </Button>
                        )}
                        <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => {
                                void frameworks.refetch();
                                void apps.refetch();
                            }}
                            disabled={refreshing}
                        >
                            <ActionMotionIcon icon={RefreshCw} size={14} motion={refreshMotion(refreshing)} />
                            刷新
                        </Button>
                    </div>
                </header>

                <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pb-6 pt-1">
                    {apps.isLoading ? (
                        <Loading text="加载实例列表…" />
                    ) : apps.error ? (
                        <p className="text-sm text-danger">读取实例失败：{apps.error}</p>
                    ) : apps.instances.length === 0 ? (
                        <PagePlaceholder className="gap-2 py-10">
                            <ActionMotionIcon
                                icon={Blocks}
                                size={28}
                                motion={RESOURCE_MOTION}
                                className="text-text-tertiary"
                            />
                            <p className="text-sm text-text-secondary">还没有应用实例</p>
                            <p className="text-xs text-text-tertiary">到「组件」页按主机新建</p>
                            {onNavigate && (
                                <Button
                                    size="sm"
                                    variant="primary"
                                    className="mt-1"
                                    onClick={() => onNavigate('components')}
                                >
                                    <ActionMotionIcon icon={Boxes} size={14} motion={EMPHASIS_MOTION} />
                                    前往组件管理
                                </Button>
                            )}
                        </PagePlaceholder>
                    ) : (
                        <InstanceList
                            instances={apps.instances}
                            manifestById={manifestById}
                            servers={servers}
                            pendingId={apps.pendingId}
                            onInstall={apps.install}
                            onStart={apps.start}
                            onStop={apps.stop}
                            onRefresh={apps.refresh}
                            onLink={(i) => setLinkInstanceId(i.id)}
                            onUnlink={(i) => apps.unlink(i.id)}
                            onOpenWebUi={(i) => void apps.openWebUi(i.id)}
                            onOpen={(i, tab) => onOpenInstance(i.id, tab)}
                            onDelete={setDeleteInstance}
                        />
                    )}
                </div>

                <AppLinkDialog
                    open={linkInstanceId !== null}
                    onOpenChange={(o) => {
                        if (!o) setLinkInstanceId(null);
                    }}
                    instanceId={linkInstanceId}
                />

                <DeleteInstanceDialog
                    instance={deleteInstance}
                    isRemoving={apps.isRemoving}
                    onClose={() => setDeleteInstance(null)}
                    onConfirm={async (removeFiles) => {
                        if (!deleteInstance) return;
                        await apps.remove({ id: deleteInstance.id, removeFiles });
                        setDeleteInstance(null);
                    }}
                />
            </div>
        </TooltipProvider>
    );
};

const Loading: React.FC<{ text: string }> = ({ text }) => (
    <PagePlaceholder className="gap-2 py-10">
        <ActionMotionIcon icon={RefreshCw} size={16} motion="spin" />
        <span className="text-sm text-text-tertiary">{text}</span>
    </PagePlaceholder>
);

interface InstanceListProps {
    instances: AppInstance[];
    manifestById: Map<string, AppFrameworkManifest>;
    servers: ReturnType<typeof useServerManager>['servers'];
    pendingId: string | null | undefined;
    onInstall: (id: string) => void;
    onStart: (id: string) => void;
    onStop: (id: string) => void;
    onRefresh: (id: string) => void;
    onLink: (i: AppInstance) => void;
    onUnlink: (i: AppInstance) => void;
    onOpenWebUi: (i: AppInstance) => void;
    onOpen: (i: AppInstance, tab?: DetailTabHint) => void;
    onDelete: (i: AppInstance) => void;
}

const InstanceList: React.FC<InstanceListProps> = (props) => {
    const m = useMotion();
    const rootRef = useRef<HTMLDivElement>(null);
    useGSAP(
        () => {
            const root = rootRef.current;
            if (!root) return;
            animateListChildrenEnter(root, props.instances.length, m);
        },
        { scope: rootRef, dependencies: [props.instances.length, m.enabled, m.level] },
    );

    return (
        <div ref={rootRef} className={gridStyles.appCardGrid}>
            {props.instances.map((i) => (
                <ListItem key={i.id} hoverable className="min-h-0 min-w-0">
                    <InstanceCard instance={i} {...props} />
                </ListItem>
            ))}
        </div>
    );
};

const InstanceCard: React.FC<InstanceListProps & { instance: AppInstance }> = ({
    instance: i,
    manifestById,
    servers,
    pendingId,
    onInstall,
    onStart,
    onStop,
    onRefresh,
    onLink,
    onUnlink,
    onOpenWebUi,
    onOpen,
    onDelete,
}) => {
    const manifest = manifestById.get(i.framework_id);
    const state = STATE_META[i.state];
    const busy = pendingId === i.id || i.state === 'installing';
    const installed = isInstalled(i);
    const running = i.state === 'running';
    const accent = i.last_error ? 'danger' : running || i.state === 'installing' ? 'brand' : 'none';

    const meta = [
        manifest?.display_name ?? i.framework_id,
        `${hostIdDisplayLabel(i.host_id, servers)} :${i.port}`,
        i.installed_version ? `v${i.installed_version}` : null,
    ]
        .filter(Boolean)
        .join(' · ');

    return (
        <article
            role="button"
            tabIndex={0}
            onClick={() => onOpen(i)}
            onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    onOpen(i);
                }
            }}
            className={cn(
                'group relative isolate flex h-full min-h-[136px] w-full min-w-0 cursor-pointer flex-col overflow-hidden',
                'rounded-md border border-border-subtle bg-surface shadow-card',
                'transition-[box-shadow,border-color] duration-200 hover:border-border hover:shadow-popover',
                'outline-none focus-visible:ring-2 focus-visible:ring-brand',
                accent === 'brand' && 'ring-1 ring-inset ring-brand/25',
                accent === 'danger' && 'ring-1 ring-inset ring-danger/25',
            )}
        >
            {accent !== 'none' && (
                <span
                    aria-hidden
                    className={cn(
                        'absolute inset-y-0 left-0 w-0.5',
                        accent === 'danger' ? 'bg-danger' : 'bg-brand',
                    )}
                />
            )}

            <div className="flex min-h-0 flex-1 flex-col justify-between px-4 pb-2.5 pt-3">
                <div className="min-w-0">
                    <h3 className="truncate font-display text-base font-semibold leading-snug text-text" title={i.display_name}>
                        {i.display_name}
                    </h3>
                    <p className="mt-0.5 truncate text-2xs text-text-tertiary" title={i.install_dir}>
                        {meta}
                    </p>
                </div>
                <div className="min-h-[1.25rem] min-w-0 text-xs leading-snug text-text-secondary">
                    {i.link ? (
                        <p className="truncate">对接 Bot {i.link.bot_id}</p>
                    ) : !installed ? (
                        <p className="truncate text-text-tertiary">安装后才能启动与配置</p>
                    ) : null}
                    {i.last_error && (
                        <p className="mt-0.5 truncate text-2xs text-danger" title={i.last_error}>
                            {i.last_error}
                        </p>
                    )}
                </div>
            </div>

            <footer
                className="flex h-11 shrink-0 items-center gap-2 border-t border-border-subtle bg-inset/35 px-3.5"
                onClick={(e) => e.stopPropagation()}
            >
                <Badge tone={state.tone} appearance="soft" dot={state.dot} className="max-w-[9.5rem] shrink-0 truncate">
                    {state.label}
                </Badge>
                <div className="flex min-w-0 flex-1 items-center justify-end gap-1">
                    {busy && <Spinner size="sm" className="mr-0.5" />}
                    {!installed && (
                        <Button size="sm" variant="primary" disabled={busy} onClick={() => onInstall(i.id)}>
                            <ActionMotionIcon icon={Download} size={13} motion={EMPHASIS_MOTION} />
                            安装
                        </Button>
                    )}
                    {installed && !running && (
                        <FooterIcon label="启动" disabled={busy} tone="brand" onClick={() => onStart(i.id)}>
                            <ActionMotionIcon icon={Play} size={15} strokeWidth={2.2} motion={EMPHASIS_MOTION} />
                        </FooterIcon>
                    )}
                    {running && (
                        <FooterIcon label="停止" disabled={busy} tone="danger" onClick={() => onStop(i.id)}>
                            <ActionMotionIcon icon={Square} size={15} strokeWidth={2.2} />
                        </FooterIcon>
                    )}
                    {installed && (
                        <FooterIcon label={i.link ? '改绑' : '对接'} disabled={busy} onClick={() => onLink(i)}>
                            <ActionMotionIcon icon={Link2} size={15} strokeWidth={2.2} />
                        </FooterIcon>
                    )}
                    {i.link && (
                        <FooterIcon label="解除对接" disabled={busy} onClick={() => onUnlink(i)}>
                            <ActionMotionIcon icon={Unlink} size={15} strokeWidth={2.2} />
                        </FooterIcon>
                    )}
                    {manifest?.has_webui && running && (
                        <FooterIcon label="打开 WebUI" onClick={() => onOpenWebUi(i)}>
                            <ActionMotionIcon icon={ExternalLink} size={15} strokeWidth={2.2} />
                        </FooterIcon>
                    )}
                    {installed && (
                        <FooterIcon label="日志" onClick={() => onOpen(i, 'log')}>
                            <ActionMotionIcon icon={ScrollText} size={15} strokeWidth={2.2} />
                        </FooterIcon>
                    )}
                    <FooterIcon label="重新探测" disabled={busy} onClick={() => onRefresh(i.id)}>
                        <ActionMotionIcon icon={RefreshCw} size={15} strokeWidth={2.2} motion={refreshMotion(busy)} />
                    </FooterIcon>
                    <FooterIcon label="删除实例" disabled={busy} tone="danger" onClick={() => onDelete(i)}>
                        <ActionMotionIcon icon={Trash2} size={15} strokeWidth={2.2} />
                    </FooterIcon>
                </div>
            </footer>
        </article>
    );
};

const FooterIcon: React.FC<{
    label: string;
    onClick: () => void;
    disabled?: boolean;
    tone?: 'neutral' | 'brand' | 'danger';
    children: React.ReactNode;
}> = ({ label, onClick, disabled, tone = 'neutral', children }) => (
    <Tooltip>
        <TooltipTrigger asChild>
            <button
                type="button"
                aria-label={label}
                disabled={disabled}
                onClick={(e) => {
                    e.stopPropagation();
                    onClick();
                }}
                className={cn(
                    'inline-flex h-8 w-8 items-center justify-center rounded-xs',
                    'transition-[color,background-color] duration-150',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                    'disabled:cursor-not-allowed disabled:opacity-40',
                    tone === 'neutral' && 'text-text-secondary hover:bg-inset hover:text-text',
                    tone === 'brand' && 'text-brand hover:bg-brand-soft',
                    tone === 'danger' && 'text-danger hover:bg-danger-soft',
                )}
            >
                {children}
            </button>
        </TooltipTrigger>
        <TooltipContent>{label}</TooltipContent>
    </Tooltip>
);

export default AppInstanceListPage;
