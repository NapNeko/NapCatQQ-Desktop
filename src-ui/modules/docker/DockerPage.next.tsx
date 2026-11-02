// 管理视图按主机和引擎挂载，避免旧弹窗操作切换后的资源。
import { useEffect, useState, type CSSProperties } from 'react';
import { Container, Disc3, HardDrive, Network, RefreshCw, Search } from 'lucide-react';
import { Button, Select } from '../../shared/ui';
import { ActionMotionIcon, ListItem, refreshMotion, Shimmer } from '../../shared/ui/motion';
import { useDocker, type DockerTab } from '../../hooks/docker/useDocker';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { openDockerDeployment } from '../../hooks/components/componentsNavigation';
import { useDockerHostSelection } from '../../hooks/docker/dockerNavigation';
import { dockerManagementSummary, imageRemoveRef } from '../../core/domain/docker/status';
import { containerEngineLabel } from '../../core/domain/docker/engine';
import { localOs } from '../../core/domain/runtime/env';
import { useContainerEngine } from '../../hooks/docker/useContainerEngine';
import { cn } from '../../shared/utils/cn';
import { DockerToolbar } from './DockerToolbar';
import { ContainerCard } from './ContainerCard';
import { ImageCard } from './ImageCard';
import { ImageRemoveDialog } from './ImageRemoveDialog';
import { ContainerLogsDialog } from './ContainerLogsDialog';
import { ContainerDetailsDialog } from './ContainerDetailsDialog';
import { DockerResourceList } from './DockerResourceList';
import { ResourceRemoveDialog, type RemovalTarget } from './ResourceRemoveDialog';
import { PagePlaceholder } from '../../shared/ui/PagePlaceholder';
import type { ContainerInfo, ImageInfo } from '../../core/ipc/types';
import type { ContainerEngine } from '../../core/ipc/generated/domain/ContainerEngine';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

const TABS = [
    { id: 'containers', label: '容器', icon: Container },
    { id: 'images', label: '镜像', icon: Disc3 },
    { id: 'networks', label: '网络', icon: Network },
    { id: 'volumes', label: '数据卷', icon: HardDrive },
] as const;

const CARD_GRID_STYLE = {
    gridTemplateColumns: 'repeat(auto-fill, minmax(min(400px, 100%), 1fr))',
} satisfies CSSProperties;

const EMPTY_HINTS: Record<DockerTab, string> = {
    containers: '在「容器部署」中新建 Bot，启动后创建对应容器。',
    images: '在「容器部署」中准备 NapCat 或 SnowLuma 镜像。',
    networks: '容器部署创建网络后，会出现在这里。',
    volumes: '容器部署或容器创建数据卷后，会出现在这里。',
};

export function DockerPageNext() {
    const { servers: allServers } = useServerManager();
    const servers = allServers.filter((server) => server.platform !== 'windows');
    const localAvailable = localOs() === 'linux';
    const { selectedHost, setSelectedHost } = useDockerHostSelection();
    const hostId =
        (selectedHost === 'local' && localAvailable) ||
        servers.some((s) => `remote:${s.id}` === selectedHost)
            ? selectedHost!
            : localAvailable
              ? 'local'
              : servers[0]
                ? `remote:${servers[0].id}`
                : '';
    if (!hostId)
        return (
            <PagePlaceholder>
                <p className="text-sm text-text-secondary">添加 Linux 远程主机后即可管理容器</p>
            </PagePlaceholder>
        );
    return (
        <DockerHostPage
            key={hostId}
            hostId={hostId}
            servers={servers}
            onChangeHost={setSelectedHost}
        />
    );
}

interface DockerHostPageProps {
    hostId: string;
    servers: ServerProfile[];
    onChangeHost: (id: string) => void;
}

export function DockerHostPage(props: DockerHostPageProps) {
    const selection = useContainerEngine(props.hostId);
    const [tab, setTab] = useState<DockerTab>('containers');
    const engine =
        selection.loading || selection.switching || selection.error ? undefined : selection.engine;
    return (
        <DockerEnginePage
            key={`${props.hostId}:${engine ?? 'unavailable'}`}
            {...props}
            tab={tab}
            onChangeTab={setTab}
            engine={engine}
            selectedEngine={selection.pendingEngine ?? selection.engine}
            engineLoading={selection.loading || selection.switching}
            engineBlocked={selection.busy || !!selection.error}
            onChangeEngine={(next) => void selection.select(next).catch(() => {})}
            onRetryEngine={() => void selection.refetch()}
        />
    );
}

function DockerEnginePage({
    hostId,
    servers,
    onChangeHost,
    tab,
    onChangeTab,
    engine,
    selectedEngine,
    engineLoading,
    engineBlocked,
    onChangeEngine,
    onRetryEngine,
}: DockerHostPageProps & {
    tab: DockerTab;
    onChangeTab: (tab: DockerTab) => void;
    engine?: ContainerEngine;
    selectedEngine?: ContainerEngine;
    engineLoading: boolean;
    engineBlocked: boolean;
    onChangeEngine: (engine: ContainerEngine) => void;
    onRetryEngine: () => void;
}) {
    const [search, setSearch] = useState('');
    const [stateFilter, setStateFilter] = useState('all');
    const docker = useDocker(engine ? hostId : '', tab, engine);
    const engineName = containerEngineLabel(engine);
    const [logs, setLogs] = useState<ContainerInfo | null>(null);
    const [details, setDetails] = useState<ContainerInfo | null>(null);
    const [remove, setRemove] = useState<RemovalTarget | null>(null);
    const [image, setImage] = useState<ImageInfo | null>(null);
    const summary = !engine
        ? null
        : docker.statusError
          ? { ready: false, label: `${engineName} 状态读取失败` }
          : docker.status
            ? dockerManagementSummary(docker.status, engineName)
            : null;
    const label = TABS.find((t) => t.id === tab)!.label;
    const term = search.trim().toLowerCase();
    const matches = (...values: string[]) =>
        values.some((value) => value.toLowerCase().includes(term));
    const containers = docker.containers.filter(
        (c) =>
            (stateFilter === 'all' || c.state === stateFilter) &&
            matches(c.name, c.id, c.image, c.status, ...c.ports),
    );
    const images = docker.images.filter((i) => matches(i.id, i.repository, i.tag));
    const networks = docker.networks.filter((n) => matches(n.name, n.id, n.driver));
    const volumes = docker.volumes.filter((v) => matches(v.name, v.driver));
    const total = {
        containers: docker.containers,
        images: docker.images,
        networks: docker.networks,
        volumes: docker.volumes,
    }[tab].length;
    const shown = { containers, images, networks, volumes }[tab].length;
    const disabled = docker.isActing || !!docker.listError || !docker.ready;
    const error = docker.statusError ?? docker.listError;

    useEffect(() => {
        setSearch('');
        setStateFilter('all');
    }, [tab]);

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <DockerToolbar
                hostId={hostId}
                servers={servers}
                onChangeHost={onChangeHost}
                engine={selectedEngine}
                onChangeEngine={onChangeEngine}
                engineDisabled={engineLoading || engineBlocked || docker.isActing}
                summary={summary}
                isProbing={engineLoading || docker.isProbing}
                resourceCount={
                    docker.ready && !docker.isLoading && !docker.listError ? total : null
                }
                resourceLabel={label}
                isRefreshing={engineLoading || docker.isRefreshing}
                onRefresh={engine ? docker.refetch : onRetryEngine}
                onOpenDeployment={() => openDockerDeployment(hostId)}
            >
                <div className="flex min-w-0 flex-wrap items-center justify-between gap-x-6 gap-y-2">
                    <nav
                        className="inline-flex w-fit max-w-full flex-wrap items-center gap-0.5 rounded-md bg-inset p-0.5"
                        aria-label="容器管理视图"
                    >
                        {TABS.map(({ id, label: title, icon: Icon }) => (
                            <button
                                key={id}
                                type="button"
                                aria-pressed={tab === id}
                                onClick={() => onChangeTab(id)}
                                className={cn(
                                    'inline-flex h-6 items-center gap-1.5 rounded-sm px-2.5 text-xs font-medium transition-colors',
                                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                                    tab === id
                                        ? 'bg-elevated text-text shadow-sm ring-1 ring-border-subtle'
                                        : 'text-text-tertiary hover:bg-elevated/35 hover:text-text',
                                )}
                            >
                                <Icon size={14} />
                                {title}
                            </button>
                        ))}
                    </nav>
                    {docker.ready && (
                        <div className="ml-auto flex min-w-0 max-w-lg flex-1 basis-72 items-center gap-2">
                            <div className="relative min-w-0 flex-1">
                                <Search
                                    size={13}
                                    aria-hidden
                                    className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-text-tertiary"
                                />
                                <input
                                    aria-label={`搜索${label}`}
                                    placeholder={`搜索${label}名称、ID${tab === 'containers' ? '或镜像' : ''}`}
                                    value={search}
                                    onChange={(event) => setSearch(event.target.value)}
                                    className="h-7 w-full min-w-0 rounded-sm border border-border-subtle bg-field pl-8 pr-2.5 text-xs text-text outline-none placeholder:text-text-tertiary focus-visible:border-brand focus-visible:ring-1 focus-visible:ring-brand"
                                />
                            </div>
                            {tab === 'containers' && (
                                <Select
                                    id="docker-state-filter"
                                    aria-label="容器状态"
                                    size="sm"
                                    className="w-28 shrink-0"
                                    value={stateFilter}
                                    onValueChange={setStateFilter}
                                    items={[
                                        { value: 'all', label: '全部状态' },
                                        { value: 'running', label: '运行中' },
                                        { value: 'exited', label: '已停止' },
                                        { value: 'created', label: '已创建' },
                                        { value: 'paused', label: '已暂停' },
                                        { value: 'restarting', label: '重启中' },
                                        { value: 'other', label: '其他状态' },
                                    ]}
                                />
                            )}
                        </div>
                    )}
                </div>
            </DockerToolbar>
            <div
                className="min-h-0 flex-1 overflow-y-auto overscroll-contain pb-6"
                aria-busy={docker.ready && docker.isLoading}
            >
                {!engine && (
                    <PagePlaceholder className="min-h-[280px] gap-3">
                        <p className="text-sm text-text-secondary">
                            {engineLoading ? '正在读取容器引擎…' : '引擎设置暂不可用'}
                        </p>
                        {!engineLoading && (
                            <Button size="sm" onClick={onRetryEngine}>
                                重试
                            </Button>
                        )}
                    </PagePlaceholder>
                )}
                {engine && error && (
                    <PagePlaceholder className="min-h-[160px] gap-3">
                        <Button size="sm" onClick={docker.refetch} disabled={docker.isRefreshing}>
                            重试
                        </Button>
                    </PagePlaceholder>
                )}
                {engine && !docker.ready && !error && (
                    <PagePlaceholder className="min-h-[280px] gap-3">
                        {docker.isProbing ? (
                            <>
                                <ActionMotionIcon
                                    icon={RefreshCw}
                                    size={24}
                                    motion={refreshMotion(true)}
                                />
                                <p className="text-sm text-text-secondary">
                                    正在连接 {engineName}…
                                </p>
                            </>
                        ) : (
                            <>
                                <Container
                                    size={28}
                                    strokeWidth={1.5}
                                    className="text-text-tertiary"
                                />
                                <div className="max-w-sm text-center">
                                    <p className="text-sm font-medium text-text">
                                        {summary?.label}
                                    </p>
                                    <p className="mt-1 text-xs leading-relaxed text-text-secondary">
                                        {docker.status?.installed
                                            ? `在部署区检查 ${engineName} 环境，完成后即可管理这台主机的资源。`
                                            : `为当前主机安装 ${engineName} 后，即可管理容器、镜像、网络和数据卷。`}
                                    </p>
                                </div>
                                <Button
                                    size="sm"
                                    variant="primary"
                                    onClick={() => openDockerDeployment(hostId)}
                                >
                                    去容器部署
                                </Button>
                            </>
                        )}
                    </PagePlaceholder>
                )}
                {docker.ready && docker.isLoading && (
                    <div role="status" aria-live="polite">
                        <span className="sr-only">正在加载{label}…</span>
                        {tab === 'networks' || tab === 'volumes' ? (
                            <DockerRowsSkeleton />
                        ) : (
                            <DockerCardsSkeleton />
                        )}
                    </div>
                )}
                {docker.ready && !docker.isLoading && !error && shown === 0 && (
                    <DockerEmptyState
                        tab={tab}
                        label={label}
                        filtered={total > 0}
                        onOpenDeployment={() => openDockerDeployment(hostId)}
                        onClearFilters={() => {
                            setSearch('');
                            setStateFilter('all');
                        }}
                    />
                )}
                {docker.ready && !docker.isLoading && tab === 'containers' && (
                    <div className="grid gap-3" style={CARD_GRID_STYLE}>
                        {containers.map((c) => (
                            <ListItem key={c.id} hoverable>
                                <ContainerCard
                                    container={c}
                                    isActing={disabled}
                                    onViewLogs={() => setLogs(c)}
                                    onDetails={() => setDetails(c)}
                                    onAction={(action) =>
                                        action === 'remove'
                                            ? setRemove({
                                                  kind: 'container',
                                                  id: c.id,
                                                  name: c.name,
                                              })
                                            : docker.containerAction({
                                                  name: c.id,
                                                  displayName: c.name,
                                                  action,
                                              })
                                    }
                                />
                            </ListItem>
                        ))}
                    </div>
                )}
                {docker.ready && !docker.isLoading && tab === 'images' && (
                    <div className="grid gap-3" style={CARD_GRID_STYLE}>
                        {images.map((i) => (
                            <ListItem key={`${i.id}:${i.repository}:${i.tag}`} hoverable>
                                <ImageCard
                                    image={i}
                                    isRemoving={disabled}
                                    onRequestRemove={() => setImage(i)}
                                />
                            </ListItem>
                        ))}
                    </div>
                )}
                {docker.ready && !docker.isLoading && (tab === 'networks' || tab === 'volumes') && (
                    <DockerResourceList
                        kind={tab === 'networks' ? 'network' : 'volume'}
                        resources={tab === 'networks' ? networks : volumes}
                        disabled={disabled}
                        onRemove={setRemove}
                    />
                )}
            </div>
            {logs && (
                <ContainerLogsDialog
                    key={logs.id}
                    hostId={hostId}
                    engine={engine}
                    id={logs.id}
                    name={logs.name}
                    onClose={() => setLogs(null)}
                />
            )}
            {details && (
                <ContainerDetailsDialog
                    key={details.id}
                    hostId={hostId}
                    engine={engine}
                    id={details.id}
                    name={details.name}
                    onClose={() => setDetails(null)}
                />
            )}
            {remove && (
                <ResourceRemoveDialog
                    key={`${remove.kind}:${remove.id}`}
                    target={remove}
                    busy={docker.isActing}
                    onClose={() => setRemove(null)}
                    onConfirm={(force) =>
                        remove.kind === 'container'
                            ? docker.containerActionAsync({
                                  name: remove.id,
                                  displayName: remove.name,
                                  action: 'remove',
                                  force,
                              })
                            : docker.removeResourceAsync({ ...remove, kind: remove.kind })
                    }
                />
            )}
            <ImageRemoveDialog
                image={image}
                isRemoving={image != null && docker.removingImageRef === imageRemoveRef(image)}
                onDismiss={() => setImage(null)}
                onConfirm={docker.removeImageAsync}
            />
        </div>
    );
}

// 卡片网格骨架：和 ContainerCard / ImageCard 同尺寸，首屏或切 tab 读取时页面不塌。
function DockerCardsSkeleton({ count = 4 }: { count?: number }) {
    return (
        <div className="grid gap-3" style={CARD_GRID_STYLE} aria-hidden>
            {Array.from({ length: count }, (_, index) => (
                <article
                    key={index}
                    className="flex min-h-[164px] flex-col rounded-md border border-border-subtle bg-surface p-3.5"
                >
                    <div className="flex items-start gap-2.5">
                        <Shimmer height={36} className="w-9 shrink-0" />
                        <div className="flex min-w-0 flex-1 flex-col gap-2 pt-0.5">
                            <Shimmer height={13} className="w-2/3" />
                            <Shimmer height={10} className="w-1/2" />
                        </div>
                    </div>
                    <div className="mt-4 flex flex-col gap-2">
                        <Shimmer height={10} className="w-full" />
                        <Shimmer height={10} className="w-4/5" />
                    </div>
                    <div className="mt-auto flex items-center gap-2 pt-4">
                        <Shimmer height={20} className="w-16" />
                        <div className="ml-auto flex items-center gap-1.5">
                            <Shimmer height={20} className="w-12" />
                            <Shimmer height={20} className="w-12" />
                            <Shimmer height={20} className="w-10" />
                        </div>
                    </div>
                </article>
            ))}
        </div>
    );
}

function DockerRowsSkeleton({ count = 4 }: { count?: number }) {
    return (
        <div className="grid gap-2 md:grid-cols-2" aria-hidden>
            {Array.from({ length: count }, (_, index) => (
                <div
                    key={index}
                    className="flex items-center gap-3 rounded-md border border-border-subtle bg-surface p-4"
                >
                    <Shimmer height={20} className="w-5 shrink-0" />
                    <div className="min-w-0 flex-1 space-y-2">
                        <Shimmer height={12} className="w-1/3" />
                        <Shimmer height={10} className="w-1/4" />
                    </div>
                    <Shimmer height={26} className="w-16 shrink-0" />
                </div>
            ))}
        </div>
    );
}

function DockerEmptyState({
    tab,
    label,
    filtered,
    onClearFilters,
    onOpenDeployment,
}: {
    tab: DockerTab;
    label: string;
    filtered: boolean;
    onClearFilters: () => void;
    onOpenDeployment: () => void;
}) {
    const Icon = TABS.find((item) => item.id === tab)!.icon;
    return (
        <PagePlaceholder className="gap-3">
            <Icon size={26} strokeWidth={1.6} className="text-text-tertiary" />
            <div>
                <p className="font-display text-md font-semibold text-text">
                    {filtered ? `没有匹配的${label}` : `这台主机上还没有${label}`}
                </p>
                <p className="mt-1 text-xs text-text-secondary">
                    {filtered ? '换个关键词，或清空筛选条件。' : EMPTY_HINTS[tab]}
                </p>
            </div>
            {filtered ? (
                <Button size="sm" variant="secondary" onClick={onClearFilters}>
                    清空筛选
                </Button>
            ) : tab === 'containers' || tab === 'images' ? (
                <Button size="sm" variant="secondary" onClick={onOpenDeployment}>
                    去容器部署
                </Button>
            ) : null}
        </PagePlaceholder>
    );
}

export default DockerPageNext;
