// Docker 页（next）：纯容器管理（仅远端 Linux 主机）。
//
// 选远端主机 → 看 Docker 状态 → 容器卡片列表（起停重启删 + 看日志）。本机
// （Windows）不用 Docker，所以这页没有"本机"选项；没有任何远端服务器时整页
// 在侧边栏就被隐藏（见 AppNext）。部署 NapCat / SnowLuma 走组件页对应框架行的
// 「Docker 部署」按钮。
//
// 严守 frontend-layering：只 import hooks / shared/ui / 自身组件 + domain
// 纯函数，不直接调 service / @tauri-apps。

import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Container, RefreshCw } from 'lucide-react';
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
import { dockerStatusSummary } from '../../core/domain/docker/status';
import { DockerToolbar } from './DockerToolbar';
import { ContainerCard } from './ContainerCard';
import { ContainerLogsDialog } from './ContainerLogsDialog';

export const DockerPageNext: React.FC = () => {
    const { servers } = useServerManager();

    // Docker 页只管远端主机（本机 Windows 不用 Docker）。默认选第一台远端；
    // 服务器列表变动后若当前选中项消失，回落到第一台。
    const [hostId, setHostId] = useState<string | null>(null);
    useEffect(() => {
        if (servers.length === 0) {
            if (hostId !== null) setHostId(null);
            return;
        }
        const stillThere = servers.some((s) => `remote:${s.id}` === hostId);
        if (!stillThere) setHostId(`remote:${servers[0].id}`);
    }, [servers, hostId]);

    const docker = useDocker(hostId ?? '');
    const summary = useMemo(
        () => (docker.status ? dockerStatusSummary(docker.status) : null),
        [docker.status],
    );
    const ready = summary?.ready ?? false;

    const [logsContainer, setLogsContainer] = useState<string | null>(null);

    return (
        <div className="flex min-h-0 flex-1 flex-col">
            <header className="flex shrink-0 items-end justify-between pb-4 pt-2">
                <div>
                    <p className="text-2xs uppercase tracking-widest text-text-tertiary">
                        docker
                    </p>
                    <h1 className="font-display text-xl font-semibold text-text">
                        容器管理
                    </h1>
                    <p className="mt-1 text-sm text-text-secondary">
                        管理远端服务器上的 Docker 容器；每张卡片可查看容器运行日志。部署过程摘要见「设置 → 日志」。
                    </p>
                </div>
                <Button
                    size="sm"
                    variant="secondary"
                    onClick={docker.refetch}
                    disabled={docker.isProbing}
                >
                    <ActionMotionIcon
                        icon={RefreshCw}
                        size={14}
                        motion={refreshMotion(docker.isProbing)}
                    />
                    刷新
                </Button>
            </header>

            <div className="shrink-0 pb-4">
                <DockerToolbar
                    hostId={hostId}
                    servers={servers}
                    onChangeHost={setHostId}
                    summary={summary}
                    isProbing={docker.isProbing}
                    containerCount={ready ? docker.containers.length : null}
                />
            </div>

            <div className="-mr-2 flex min-h-0 flex-1 flex-col overflow-y-auto pb-6 pr-2">
                {ready && (
                    <ContainerList docker={docker} onViewLogs={setLogsContainer} />
                )}
            </div>

            {logsContainer && (
                <ContainerLogsDialog
                    name={logsContainer}
                    fetchLogs={docker.fetchLogs}
                    onClose={() => setLogsContainer(null)}
                />
            )}
        </div>
    );
};

// ─── 容器列表区 ─────────────────────────────────────────────────────

const ContainerList: React.FC<{
    docker: ReturnType<typeof useDocker>;
    onViewLogs: (name: string) => void;
}> = ({ docker, onViewLogs }) => {
    if (docker.isLoadingList) {
        return (
            <div className="flex items-center gap-2 rounded-md bg-inset/40 p-6 text-text-tertiary">
                <ActionMotionIcon icon={RefreshCw} size={16} motion="spin" />
                <span className="text-sm">加载容器列表…</span>
            </div>
        );
    }
    if (docker.containers.length === 0) {
        return (
            <div className="flex flex-col items-center gap-2 rounded-md bg-inset/30 p-10 text-center">
                <ActionMotionIcon
                    icon={Container}
                    size={28}
                    motion={RESOURCE_MOTION}
                    className="text-text-tertiary"
                />
                <p className="text-sm text-text-secondary">这台主机上还没有容器</p>
                <p className="text-xs text-text-tertiary">
                    去组件页的「Docker 部署」起一个 NapCat / SnowLuma
                </p>
            </div>
        );
    }
    return (
        <DockerContainerGrid docker={docker} onViewLogs={onViewLogs} />
    );
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
        {
            scope: containerRef,
            dependencies: [docker.containers.length, m.enabled, m.level],
        },
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

export default DockerPageNext;
