// 运行宿主、部署方式与就绪探测；探测状态不触发其他表单区域渲染。
import { useMemo, type ComponentType } from 'react';
import { Box, Monitor, Server, Terminal, type LucideProps } from 'lucide-react';
import { FormSection, Select } from '../../../../shared/ui';
import { ExpandPresence } from '../../../../shared/ui/motion/ExpandPresence';
import { useServers } from '../../../../hooks/remote/useServers';
import { useDockerHosts } from '../../../../hooks/docker/useDockerHosts';
import {
    useComponentNames,
    useRuntimeReadiness,
} from '../../../../hooks/components/useRuntimeReadiness';
import { useIsHostReachable } from '../../../../hooks/remote/useIsHostReachable';
import { describeBlocking } from '../../../../core/domain/components/readiness';
import {
    RUNTIME_TARGET_REMOTE_PLACEHOLDER,
    isRuntimeTargetConcreteRemote,
    isRuntimeTargetLocal,
    remoteHostIdFromRuntimeTarget,
    runtimeModeForTarget,
    serverProfileIdFromRuntimeTarget,
} from '../../../../core/domain/bot/runtime-target';
import { dockerHostTargetKey } from '../../../../core/domain/docker/engine';
import { localOs } from '../../../../core/domain/runtime/env';
import { cn } from '../../../../shared/utils/cn';
import type { ContainerEngine } from '../../../../core/ipc/generated/domain/ContainerEngine';
import type { DeploymentType } from '../../../../core/ipc/generated/domain/DeploymentType';
import type { IdentityTabProps } from './IdentityTab';
import {
    DockerReadinessLine,
    InlineNotice,
    SceneExplainerPopover,
    SelectionDot,
    SnowLumaStartModeBlock,
} from './identityParts';

const RUNTIME_ITEMS: ReadonlyArray<{
    value: 'local' | 'remote';
    label: string;
    summary: string;
    icon: ComponentType<LucideProps>;
}> = [
    {
        value: 'local',
        label: '本机',
        summary: '当前桌面主机，Linux 可选 Docker / Podman',
        icon: Monitor,
    },
    {
        value: 'remote',
        label: '远程主机',
        summary: '经 SSH 部署并托管 Linux 主机',
        icon: Server,
    },
];

const DEPLOYMENT_ITEMS: ReadonlyArray<{
    value: DeploymentType;
    label: string;
    icon: ComponentType<LucideProps>;
}> = [
    { value: 'native', label: '直接运行', icon: Terminal },
    { value: 'docker', label: '容器', icon: Box },
];

export function IdentityRuntimeSection({
    data,
    onChange,
    isRunning,
    containerEngine,
    onContainerEngineChange,
    containerEngineLocked = false,
}: Omit<IdentityTabProps, 'isEditMode'>) {
    // 此处只读档案，不创建主机增删改和连接测试的 mutation observers。
    const { data: servers = [], isLoading: serversLoading } = useServers();
    const isRemote = !isRuntimeTargetLocal(data.runtime_target);
    const localDockerAllowed = localOs() === 'linux';
    const runtimeMode = runtimeModeForTarget(data.runtime_target);

    const serverItems = useMemo(
        () => servers.map((s) => ({ value: s.id, label: `${s.name} · ${s.host}` })),
        [servers],
    );

    const hasRemoteHosts = servers.length > 0;

    const deploymentType: DeploymentType = data.deploymentType === 'docker' ? 'docker' : 'native';

    const remoteHostId = useMemo(
        () => (isRemote ? remoteHostIdFromRuntimeTarget(data.runtime_target) : null),
        [isRemote, data.runtime_target],
    );

    const dockerHostId = isRemote ? remoteHostId : localDockerAllowed ? 'local' : null;
    const dockerAllowed = isRemote
        ? servers.find((server) => server.id === data.runtime_target)?.platform !== 'windows'
        : localDockerAllowed;
    const dockerHostIds = useMemo(
        () =>
            deploymentType === 'docker' && dockerHostId
                ? [{ hostId: dockerHostId, engine: containerEngine }]
                : [],
        [deploymentType, dockerHostId, containerEngine],
    );
    const dockerProbeKey = dockerHostId
        ? dockerHostTargetKey({ hostId: dockerHostId, engine: containerEngine })
        : '';
    const { statusByHost, probingByHost, imageReadyByHost } = useDockerHosts(dockerHostIds);

    // 直接运行的框架 + 依赖状态由后端解析；SnowLuma Full/Lite 也由后端按设置 / 库存判定
    const readiness = useRuntimeReadiness(
        deploymentType === 'native' ? (isRemote ? remoteHostId : 'local') : null,
        data.backend_type,
    );
    const componentNames = useComponentNames();

    const remoteReachable = useIsHostReachable(remoteHostId);
    const remoteTransportFailed = isRemote && remoteHostId != null && !remoteReachable;

    const missingDirectRunNotice = useMemo(() => {
        if (!isRemote || deploymentType !== 'native' || !remoteHostId) {
            return null;
        }
        if (!readiness.readiness) return null;
        const blocking = describeBlocking(readiness.readiness, componentNames);
        return blocking ? `远程主机${blocking}，请到「组件」页安装` : null;
    }, [isRemote, deploymentType, remoteHostId, readiness.readiness, componentNames]);

    const onRuntimeModeChange = (mode: 'local' | 'remote') => {
        if (mode === 'local') {
            onChange({
                runtime_target: 'local',
                deploymentType: localDockerAllowed ? deploymentType : 'native',
            });
        } else {
            const only = servers.length === 1 ? servers[0].id : RUNTIME_TARGET_REMOTE_PLACEHOLDER;
            onChange({
                runtime_target: only,
                ...(servers.find((server) => server.id === only)?.platform === 'windows'
                    ? { deploymentType: 'native' }
                    : {}),
            });
        }
    };

    const remoteHostSelectValue: string | undefined = (() => {
        const profileId = serverProfileIdFromRuntimeTarget(data.runtime_target);
        if (!profileId) return undefined;
        if (servers.some((s) => s.id === profileId)) return profileId;
        return undefined;
    })();

    const showDockerBlock =
        deploymentType === 'docker' &&
        (data.backend_type === 'napcat' || data.backend_type === 'snowluma') &&
        dockerHostId != null;

    const dockerFlavorLabel = data.backend_type === 'snowluma' ? 'SnowLuma' : 'NapCat';
    const localBlocking = readiness.readiness
        ? describeBlocking(readiness.readiness, componentNames)
        : null;
    const localNotice = localBlocking
        ? `本机${localBlocking}，请到「组件」页安装后再使用本机直接运行`
        : readiness.probing && !readiness.readiness
          ? '正在检测本机运行时组件…'
          : null;
    const remoteNotice = remoteTransportFailed
        ? '远端主机不可达，保存后无法启动。请先在「远程主机」页恢复连接。'
        : missingDirectRunNotice;

    // 当前运行场景的机制说明，收进 Popover 默认折叠，不常驻长文。
    const sceneExplainer = useMemo(() => {
        if (!isRemote || !isRuntimeTargetConcreteRemote(data.runtime_target)) {
            return null;
        }
        if (data.backend_type === 'napcat' && deploymentType === 'native') {
            return 'Bot 启动后桌面经 SSH 隧道打开远端 NapCat WebUI（本机 6099）并轮询登录态；日志打出 WebUI 地址前先用持久化 token 尝试。';
        }
        if (data.backend_type === 'snowluma' && deploymentType === 'native') {
            return '桌面把远端 5099 / 6081 转发到本机回环，列表可开 WebUI 与 noVNC 扫码；登录密码来自远端 secret，打开时复制到剪贴板。冷/热启动由「启动模式」决定。';
        }
        if (data.backend_type === 'snowluma' && deploymentType === 'docker') {
            return '容器内自带 QQ 图形环境，登录在容器内完成；VNC 与 WebUI 密码由桌面写入 compose。列表在隧道就绪后可开 noVNC 扫码与 WebUI。';
        }
        return null;
    }, [isRemote, data.runtime_target, data.backend_type, deploymentType]);

    return (
        <div data-tour-id="bot-runtime-section">
            <FormSection
                title="运行场景"
                layout="none"
                description="Linux 主机支持直接运行与 Docker / Podman；Windows 使用直接运行"
                actions={
                    sceneExplainer ? (
                        <SceneExplainerPopover>{sceneExplainer}</SceneExplainerPopover>
                    ) : undefined
                }
            >
                <div className="flex flex-col gap-1.5">
                    <span className="text-xs font-medium text-text-secondary">运行宿主</span>
                    <div
                        role="radiogroup"
                        aria-label="运行宿主"
                        className="grid grid-cols-1 gap-3 sm:grid-cols-2"
                    >
                        {RUNTIME_ITEMS.map((item) => {
                            const selected = runtimeMode === item.value;
                            const Icon = item.icon;
                            return (
                                <button
                                    key={item.value}
                                    type="button"
                                    role="radio"
                                    aria-checked={selected}
                                    onClick={() => onRuntimeModeChange(item.value)}
                                    className={cn(
                                        'flex h-full min-w-0 items-center justify-between gap-3 rounded-md border px-3.5 py-2.5 text-left transition-all',
                                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                                        selected
                                            ? 'border-brand/70 bg-brand-soft/20 shadow-xs ring-1 ring-brand/25'
                                            : 'border-border-subtle bg-field/45 hover:border-border hover:bg-field/80',
                                    )}
                                >
                                    <div className="flex min-w-0 items-center gap-2.5">
                                        <span
                                            className={cn(
                                                'flex h-8 w-8 shrink-0 items-center justify-center rounded-sm border transition-colors',
                                                selected
                                                    ? 'border-brand/30 bg-brand-soft text-brand'
                                                    : 'border-border-subtle bg-inset text-text-secondary',
                                            )}
                                        >
                                            <Icon size={15} strokeWidth={2.1} />
                                        </span>
                                        <div className="min-w-0">
                                            <div className="text-sm font-medium text-text">
                                                {item.label}
                                            </div>
                                            <div className="truncate text-2xs text-text-tertiary">
                                                {item.summary}
                                            </div>
                                        </div>
                                    </div>
                                    <SelectionDot selected={selected} />
                                </button>
                            );
                        })}
                    </div>
                </div>

                <ExpandPresence visible={isRemote || localDockerAllowed}>
                    <div className="pt-3">
                        <div className="flex flex-col rounded-md border border-border-subtle/80 bg-field/30 p-3.5">
                            <ExpandPresence
                                visible={isRemote && !hasRemoteHosts && !serversLoading}
                            >
                                <div className="pb-3">
                                    <InlineNotice tone="warn">
                                        请先在「远程主机」页添加 SSH 主机；当前配置仍按远程保存
                                    </InlineNotice>
                                </div>
                            </ExpandPresence>
                            <div
                                className={cn(
                                    'grid grid-cols-1 gap-3',
                                    isRemote &&
                                        hasRemoteHosts &&
                                        serverItems.length > 0 &&
                                        'sm:grid-cols-2',
                                )}
                            >
                                {isRemote && hasRemoteHosts && serverItems.length > 0 && (
                                    <Select
                                        label="目标远程主机"
                                        items={serverItems}
                                        value={remoteHostSelectValue}
                                        onValueChange={(v) =>
                                            onChange({
                                                runtime_target: v,
                                                ...(servers.find((server) => server.id === v)
                                                    ?.platform === 'windows'
                                                    ? { deploymentType: 'native' }
                                                    : {}),
                                            })
                                        }
                                        placeholder="选择主机"
                                    />
                                )}

                                <div className="flex flex-col gap-1.5">
                                    <span className="text-xs font-medium text-text-secondary">
                                        启动方式
                                    </span>
                                    <div
                                        role="radiogroup"
                                        aria-label="启动方式"
                                        className="grid h-[38px] grid-cols-2 gap-1 rounded-sm border border-border-subtle bg-inset p-0.5"
                                    >
                                        {DEPLOYMENT_ITEMS.map((item) => {
                                            const selected = deploymentType === item.value;
                                            const Icon = item.icon;
                                            return (
                                                <button
                                                    key={item.value}
                                                    type="button"
                                                    role="radio"
                                                    aria-checked={selected}
                                                    disabled={
                                                        item.value === 'docker' && !dockerAllowed
                                                    }
                                                    onClick={() =>
                                                        onChange({ deploymentType: item.value })
                                                    }
                                                    className={cn(
                                                        'inline-flex items-center justify-center gap-1.5 rounded-xs px-2.5 text-xs font-medium transition-all',
                                                        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                                                        selected
                                                            ? 'border border-border/50 bg-surface text-text shadow-xs'
                                                            : 'border border-transparent text-text-tertiary hover:text-text',
                                                    )}
                                                >
                                                    <Icon
                                                        size={13}
                                                        className={
                                                            selected
                                                                ? 'text-brand'
                                                                : 'text-text-tertiary'
                                                        }
                                                    />
                                                    <span>{item.label}</span>
                                                </button>
                                            );
                                        })}
                                    </div>
                                </div>
                            </div>

                            <ExpandPresence visible={!!showDockerBlock}>
                                <div className="pt-3">
                                    {onContainerEngineChange && (
                                        <label className="mb-3 flex items-center gap-3 text-xs text-text-secondary">
                                            容器引擎
                                            <Select
                                                aria-label="容器引擎"
                                                value={containerEngine ?? 'default'}
                                                disabled={containerEngineLocked || isRunning}
                                                items={[
                                                    { value: 'default', label: '主机当前引擎' },
                                                    { value: 'docker', label: 'Docker' },
                                                    { value: 'podman', label: 'Podman' },
                                                ]}
                                                onValueChange={(value) =>
                                                    onContainerEngineChange(
                                                        value === 'default'
                                                            ? undefined
                                                            : (value as ContainerEngine),
                                                    )
                                                }
                                            />
                                        </label>
                                    )}
                                    <DockerReadinessLine
                                        engine={containerEngine}
                                        flavorLabel={dockerFlavorLabel}
                                        status={
                                            dockerHostId ? statusByHost[dockerProbeKey] : undefined
                                        }
                                        probing={
                                            dockerHostId
                                                ? (probingByHost[dockerProbeKey] ?? false)
                                                : false
                                        }
                                        imageReady={
                                            dockerHostId
                                                ? imageReadyByHost[dockerProbeKey]?.[
                                                      data.backend_type === 'snowluma'
                                                          ? 'snowluma'
                                                          : 'napcat'
                                                  ]
                                                : undefined
                                        }
                                    />
                                </div>
                            </ExpandPresence>

                            <ExpandPresence
                                visible={
                                    deploymentType === 'native' && !!remoteHostId && !!remoteNotice
                                }
                            >
                                <div className="pt-3">
                                    <InlineNotice tone={remoteTransportFailed ? 'danger' : 'warn'}>
                                        {remoteNotice}
                                    </InlineNotice>
                                </div>
                            </ExpandPresence>
                        </div>
                    </div>
                </ExpandPresence>

                <ExpandPresence visible={!isRemote && deploymentType === 'native' && !!localNotice}>
                    <div className="pt-3">
                        <InlineNotice tone={localBlocking ? 'warn' : 'neutral'}>
                            {localNotice}
                        </InlineNotice>
                    </div>
                </ExpandPresence>

                <ExpandPresence
                    visible={data.backend_type === 'snowluma' && deploymentType !== 'docker'}
                >
                    <div className="flex flex-col gap-1.5 pt-3">
                        <span className="text-xs font-medium text-text-secondary">
                            SnowLuma 启动模式
                        </span>
                        <SnowLumaStartModeBlock data={data} onChange={onChange} />
                    </div>
                </ExpandPresence>
            </FormSection>
        </div>
    );
}
