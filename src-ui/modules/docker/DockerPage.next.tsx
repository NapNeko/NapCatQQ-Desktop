// Docker 页（next）：远端主机上的容器与镜像管理。
//
// 选远端主机 → 看 Docker 状态 → Tab 切换容器 / 镜像列表。拉取镜像仍在组件页。

import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Container, Disc3, RefreshCw } from 'lucide-react';
import { useGSAP } from '@gsap/react';
import { animateListChildrenEnter } from '../../shared/ui/motion/listEnter';
import { Button } from '../../shared/ui';
import {
    ActionMotionIcon,
    ListItem,
    RESOURCE_MOTION,
    refreshMotion,
} from '../../shared/ui/motion';
import { useMotion } from '../../hooks/preferences/useMotion';
import { useDocker } from '../../hooks/docker/useDocker';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { dockerStatusSummary, imageRemoveRef } from '../../core/domain/docker/status';
import { cn } from '../../shared/utils/cn';
import { DockerToolbar } from './DockerToolbar';
import { ContainerCard } from './ContainerCard';
import { ImageCard } from './ImageCard';
import { ImageRemoveDialog } from './ImageRemoveDialog';
import { ContainerLogsDialog } from './ContainerLogsDialog';
import { PagePlaceholder } from '../../shared/ui/PagePlaceholder';

import type { ImageInfo } from '../../core/ipc/types';

type DockerTab = 'containers' | 'images';

export const DockerPageNext: React.FC = () => {
    const { servers } = useServerManager();
    const [tab, setTab] = useState<DockerTab>('containers');

    const [hostId, setHostId] = useState<string | null>(null);
    useEffect(() => {
        if (servers.length === 0) {
            if (hostId !== null) setHostId(null);
            return;
        }
        const stillThere = servers.some((s) => `remote:${s.id}` === hostId);
        if (!stillThere) setHostId(`remote:${servers[0].id}`);
    }, [servers, hostId]);

    const docker = useDocker(hostId ?? '', tab);
    const summary = useMemo(
        () => (docker.status ? dockerStatusSummary(docker.status) : null),
        [docker.status],
    );
    const ready = summary?.ready ?? false;

    const [logsContainer, setLogsContainer] = useState<string | null>(null);
    const [imagePendingRemove, setImagePendingRemove] = useState<ImageInfo | null>(null);

    const pendingRemoveRef = imagePendingRemove ? imageRemoveRef(imagePendingRemove) : null;

    const resourceCount =
        tab === 'containers' ? (ready ? docker.containers.length : null) : ready ? docker.images.length : null;
    const resourceLabel = tab === 'containers' ? '容器' : '镜像';

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">docker</p>
                    <h1 className="font-display text-xl font-semibold text-text">Docker 管理</h1>
                    <p className="mt-1 text-sm text-text-secondary">
                        管理远端服务器上的容器与本地镜像；部署与拉取框架镜像请前往组件页。
                    </p>
                </div>
                <Button size="sm" variant="secondary" onClick={docker.refetch} disabled={docker.isProbing}>
                    <ActionMotionIcon icon={RefreshCw} size={14} motion={refreshMotion(docker.isProbing)} />
                    刷新
                </Button>
            </header>

            <div className="shrink-0 space-y-3 pb-4">
                <DockerTabBar tab={tab} onChange={setTab} />
                <DockerToolbar
                    hostId={hostId}
                    servers={servers}
                    onChangeHost={setHostId}
                    summary={summary}
                    isProbing={docker.isProbing}
                    resourceCount={resourceCount}
                    resourceLabel={resourceLabel}
                />
            </div>

            <div className="-mr-2 flex min-h-0 flex-1 flex-col overflow-y-auto pb-6 pr-2">
                {ready && tab === 'containers' && (
                    <ContainerList docker={docker} onViewLogs={setLogsContainer} />
                )}
                {ready && tab === 'images' && (
                    <ImageList docker={docker} onRequestRemoveImage={setImagePendingRemove} />
                )}
            </div>

            {logsContainer && (
                <ContainerLogsDialog
                    name={logsContainer}
                    fetchLogs={docker.fetchLogs}
                    onClose={() => setLogsContainer(null)}
                />
            )}

            <ImageRemoveDialog
                image={imagePendingRemove}
                isRemoving={pendingRemoveRef != null && docker.removingImageRef === pendingRemoveRef}
                onDismiss={() => setImagePendingRemove(null)}
                onConfirm={(req) => docker.removeImageAsync(req)}
            />
        </div>
    );
};

const DockerTabBar: React.FC<{ tab: DockerTab; onChange: (t: DockerTab) => void }> = ({
    tab,
    onChange,
}) => (
    <div
        className="inline-flex rounded-md border border-border-subtle bg-inset/30 p-0.5"
        role="tablist"
        aria-label="Docker 管理视图"
    >
        <TabButton active={tab === 'containers'} onClick={() => onChange('containers')}>
            <ActionMotionIcon icon={Container} size={14} motion={RESOURCE_MOTION} />
            容器
        </TabButton>
        <TabButton active={tab === 'images'} onClick={() => onChange('images')}>
            <ActionMotionIcon icon={Disc3} size={14} motion={RESOURCE_MOTION} />
            镜像
        </TabButton>
    </div>
);

const TabButton: React.FC<{
    active: boolean;
    onClick: () => void;
    children: React.ReactNode;
}> = ({ active, onClick, children }) => (
    <button
        type="button"
        role="tab"
        aria-selected={active}
        onClick={onClick}
        className={cn(
            'inline-flex items-center gap-1.5 rounded-sm px-3 py-1.5 text-sm transition-colors',
            active ? 'bg-surface text-text shadow-card' : 'text-text-secondary hover:text-text',
        )}
    >
        {children}
    </button>
);

const ContainerList: React.FC<{
    docker: ReturnType<typeof useDocker>;
    onViewLogs: (name: string) => void;
}> = ({ docker, onViewLogs }) => {
    if (docker.isLoadingList) {
        return (
            <PagePlaceholder className="gap-2 py-12">
                <ActionMotionIcon icon={RefreshCw} size={16} motion="spin" />
                <span className="text-sm text-text-tertiary">加载容器列表…</span>
            </PagePlaceholder>
        );
    }
    if (docker.containers.length === 0) {
        return (
            <PagePlaceholder className="gap-2">
                <ActionMotionIcon icon={Container} size={28} motion={RESOURCE_MOTION} className="text-text-tertiary" />
                <p className="text-sm text-text-secondary">这台主机上还没有容器</p>
                <p className="text-xs text-text-tertiary">去组件页的「Docker 部署」起一个 NapCat / SnowLuma</p>
            </PagePlaceholder>
        );
    }
    return <DockerContainerGrid docker={docker} onViewLogs={onViewLogs} />;
};

const ImageList: React.FC<{
    docker: ReturnType<typeof useDocker>;
    onRequestRemoveImage: (image: ImageInfo) => void;
}> = ({ docker, onRequestRemoveImage }) => {
    if (docker.isLoadingImages) {
        return (
            <PagePlaceholder className="gap-2 py-12">
                <ActionMotionIcon icon={RefreshCw} size={16} motion="spin" />
                <span className="text-sm text-text-tertiary">加载镜像列表…</span>
            </PagePlaceholder>
        );
    }
    if (docker.images.length === 0) {
        return (
            <PagePlaceholder className="gap-2">
                <ActionMotionIcon icon={Disc3} size={28} motion={RESOURCE_MOTION} className="text-text-tertiary" />
                <p className="text-sm text-text-secondary">这台主机上还没有本地镜像</p>
                <p className="text-xs text-text-tertiary">在组件页拉取 NapCat / SnowLuma 框架镜像后会出现在这里</p>
            </PagePlaceholder>
        );
    }
    return <DockerImageGrid docker={docker} onRequestRemoveImage={onRequestRemoveImage} />;
};

const DockerContainerGrid: React.FC<{
    docker: ReturnType<typeof useDocker>;
    onViewLogs: (name: string) => void;
}> = ({ docker, onViewLogs }) => {
    const m = useMotion();
    const containerRef = useRef<HTMLDivElement>(null);

    useGSAP(
        () => {
            const root = containerRef.current;
            if (!root) return;
            animateListChildrenEnter(root, docker.containers.length, m);
        },
        { scope: containerRef, dependencies: [docker.containers.length, m.enabled, m.level] },
    );

    return (
        <div
            ref={containerRef}
            className="grid gap-3"
            style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(min(380px, 100%), 1fr))' }}
        >
            {docker.containers.map((c) => (
                <ListItem key={c.id} hoverable>
                    <ContainerCard
                        container={c}
                        isActing={docker.isActing}
                        onAction={(action) => docker.containerAction({ name: c.name, action })}
                        onViewLogs={() => onViewLogs(c.name)}
                    />
                </ListItem>
            ))}
        </div>
    );
};

const DockerImageGrid: React.FC<{
    docker: ReturnType<typeof useDocker>;
    onRequestRemoveImage: (image: ImageInfo) => void;
}> = ({ docker, onRequestRemoveImage }) => {
    const m = useMotion();
    const containerRef = useRef<HTMLDivElement>(null);

    useGSAP(
        () => {
            const root = containerRef.current;
            if (!root) return;
            animateListChildrenEnter(root, docker.images.length, m);
        },
        { scope: containerRef, dependencies: [docker.images.length, m.enabled, m.level] },
    );

    return (
        <div
            ref={containerRef}
            className="grid gap-3"
            style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(min(380px, 100%), 1fr))' }}
        >
            {docker.images.map((img) => {
                const ref = imageRemoveRef(img);
                return (
                    <ListItem key={`${img.id}-${img.repository}-${img.tag}`} hoverable>
                        <ImageCard
                            image={img}
                            isRemoving={docker.removingImageRef === ref}
                            onRequestRemove={() => onRequestRemoveImage(img)}
                        />
                    </ListItem>
                );
            })}
        </div>
    );
};

export default DockerPageNext;