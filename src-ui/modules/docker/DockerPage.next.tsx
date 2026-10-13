// Docker 管理页按主机挂载，避免旧弹窗对新主机执行操作。
import { useEffect, useState } from 'react';
import { Container, Disc3, HardDrive, Network, RefreshCw } from 'lucide-react';
import { Button, Checkbox, Select } from '../../shared/ui';
import { ActionMotionIcon, ListItem, refreshMotion } from '../../shared/ui/motion';
import { useDocker, type DockerTab } from '../../hooks/docker/useDocker';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { dockerManagementSummary, imageRemoveRef } from '../../core/domain/docker/status';
import { errorText } from '../../core/domain/errors';
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
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

const TABS = [
    { id: 'containers', label: '容器', icon: Container },
    { id: 'images', label: '镜像', icon: Disc3 },
    { id: 'networks', label: '网络', icon: Network },
    { id: 'volumes', label: '数据卷', icon: HardDrive },
] as const;

export function DockerPageNext() {
    const { servers } = useServerManager();
    const [selectedHost, setSelectedHost] = useState<string | null>(null);
    const hostId = servers.some((s) => `remote:${s.id}` === selectedHost)
        ? selectedHost!
        : servers[0]
          ? `remote:${servers[0].id}`
          : '';
    if (!hostId)
        return (
            <PagePlaceholder>
                <p className="text-sm text-text-secondary">添加远程主机后即可管理 Docker</p>
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

export function DockerHostPage({
    hostId,
    servers,
    onChangeHost,
}: {
    hostId: string;
    servers: ServerProfile[];
    onChangeHost: (id: string) => void;
}) {
    const [tab, setTab] = useState<DockerTab>('containers');
    const [search, setSearch] = useState('');
    const [stateFilter, setStateFilter] = useState('all');
    const [autoRefresh, setAutoRefresh] = useState(false);
    const docker = useDocker(hostId, tab, autoRefresh);
    const [logs, setLogs] = useState<ContainerInfo | null>(null);
    const [details, setDetails] = useState<ContainerInfo | null>(null);
    const [remove, setRemove] = useState<RemovalTarget | null>(null);
    const [image, setImage] = useState<ImageInfo | null>(null);
    const summary = docker.statusError
        ? { ready: false, label: 'Docker 状态读取失败' }
        : docker.status
          ? dockerManagementSummary(docker.status)
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
            <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">docker</p>
                    <h1 className="font-display text-xl font-semibold text-text">Docker 管理</h1>
                </div>
                <div className="flex items-center gap-3">
                    <Checkbox
                        id="docker-auto"
                        label="自动刷新"
                        checked={autoRefresh}
                        onCheckedChange={setAutoRefresh}
                    />
                    <Button
                        size="sm"
                        variant="secondary"
                        onClick={docker.refetch}
                        disabled={docker.isRefreshing}
                    >
                        <ActionMotionIcon
                            icon={RefreshCw}
                            size={14}
                            motion={refreshMotion(docker.isRefreshing)}
                        />
                        刷新
                    </Button>
                </div>
            </header>
            <div className="shrink-0 space-y-3 pb-4">
                <nav className="flex flex-wrap gap-1" aria-label="Docker 管理视图">
                    {TABS.map(({ id, label: title, icon: Icon }) => (
                        <button
                            key={id}
                            type="button"
                            aria-pressed={tab === id}
                            onClick={() => setTab(id)}
                            className={cn(
                                'inline-flex items-center gap-1.5 rounded-sm px-3 py-2 text-sm transition-colors',
                                tab === id
                                    ? 'bg-surface text-text shadow-card'
                                    : 'text-text-secondary hover:text-text',
                            )}
                        >
                            <Icon size={14} />
                            {title}
                        </button>
                    ))}
                </nav>
                <DockerToolbar
                    hostId={hostId}
                    servers={servers}
                    onChangeHost={onChangeHost}
                    summary={summary}
                    isProbing={docker.isProbing}
                    resourceCount={
                        docker.ready && !docker.isLoading && !docker.listError ? total : null
                    }
                    resourceLabel={label}
                    showSetupHint={!docker.statusError && !!docker.status && !docker.ready}
                />
                {docker.ready && (
                    <div className="flex flex-wrap items-center gap-3">
                        <input
                            aria-label={`搜索${label}`}
                            placeholder={`搜索${label}名称、ID${tab === 'containers' ? '或镜像' : ''}`}
                            value={search}
                            onChange={(event) => setSearch(event.target.value)}
                            className="min-w-0 flex-1 rounded-sm border border-border-subtle bg-field px-3 py-2 text-sm text-text"
                        />
                        {tab === 'containers' && (
                            <Select
                                id="docker-state-filter"
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
                {error && (
                    <div
                        role="alert"
                        className="rounded-md border border-danger/30 bg-danger-soft p-3 text-sm"
                    >
                        <p className="font-medium text-danger">
                            {docker.statusError ? '无法读取 Docker 状态' : `无法刷新${label}`}
                        </p>
                        <p className="mt-1 break-all text-text-secondary">{errorText(error)}</p>
                        <Button
                            size="sm"
                            variant="secondary"
                            onClick={docker.refetch}
                            disabled={docker.isRefreshing}
                            className="mt-2"
                        >
                            重试
                        </Button>
                    </div>
                )}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto pb-6">
                {docker.ready && docker.isLoading && (
                    <PagePlaceholder>
                        <p className="text-sm text-text-tertiary">加载{label}…</p>
                    </PagePlaceholder>
                )}
                {docker.ready && !docker.isLoading && !error && shown === 0 && (
                    <PagePlaceholder>
                        <p className="text-sm text-text-secondary">
                            {total === 0 ? `这台主机上还没有${label}` : `没有匹配的${label}`}
                        </p>
                    </PagePlaceholder>
                )}
                {docker.ready && !docker.isLoading && tab === 'containers' && (
                    <div
                        className="grid gap-3"
                        style={{
                            gridTemplateColumns: 'repeat(auto-fill, minmax(min(400px, 100%), 1fr))',
                        }}
                    >
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
                    <div
                        className="grid gap-3"
                        style={{
                            gridTemplateColumns: 'repeat(auto-fill, minmax(min(380px, 100%), 1fr))',
                        }}
                    >
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
                    id={logs.id}
                    name={logs.name}
                    onClose={() => setLogs(null)}
                />
            )}
            {details && (
                <ContainerDetailsDialog
                    key={details.id}
                    hostId={hostId}
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

export default DockerPageNext;
