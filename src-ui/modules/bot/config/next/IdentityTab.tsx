import type { ContainerEngine } from '../../../../core/ipc/generated/domain/ContainerEngine';
import { dockerHostTargetKey } from '../../../../core/domain/docker/engine';
// 身份 Tab：账号、底座、运行宿主与自愈。

import { useEffect, useMemo, type ComponentType, type ReactNode } from 'react';
import {
    AlertTriangle,
    Box,
    CheckCircle2,
    CircleHelp,
    Flame,
    Info,
    Monitor,
    Server,
    Snowflake,
    Terminal,
    type LucideProps,
} from 'lucide-react';
import {
    Badge,
    FormSection,
    NumberField,
    Popover,
    PopoverContent,
    PopoverTrigger,
    Select,
    Switch,
    TextField,
} from '../../../../shared/ui';
import { ExpandPresence } from '../../../../shared/ui/motion/ExpandPresence';
import { AutoRestartScheduleEditor } from './AutoRestartScheduleEditor';
import { useServerManager } from '../../../../hooks/remote/useServerManager';
import { useFeatures } from '../../../../hooks/preferences/featureTogglesStore';
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
import { dockerReadinessNotice } from '../../../../core/domain/bot/docker-start-gate';
import { localOs } from '../../../../core/domain/runtime/env';
import { cn } from '../../../../shared/utils/cn';
import type { BotBasicConfig } from '../../../../core/ipc/generated/domain/BotBasicConfig';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
import type { DeploymentType } from '../../../../core/ipc/generated/domain/DeploymentType';
import type { AutoRestartSchedule } from '../../../../core/ipc/generated/domain/AutoRestartSchedule';
import type { SnowLumaStartMode } from '../../../../core/ipc/generated/domain/SnowLumaStartMode';

interface IdentityTabProps {
    data: BotBasicConfig;
    onChange: (patch: Partial<BotBasicConfig>) => void;
    isEditMode: boolean;
    isRunning: boolean;
    containerEngine?: ContainerEngine;
    onContainerEngineChange?: (engine: ContainerEngine | undefined) => void;
    containerEngineLocked?: boolean;
}

const BACKEND_ITEMS: ReadonlyArray<{
    value: BackendType;
    label: string;
    badge: string;
    summary: string;
    tone: 'brand' | 'info';
}> = [
    {
        value: 'napcat',
        label: 'NapCat',
        badge: '不带 QQ GUI',
        summary: '运行时关闭 QQ 客户端窗口，纯后台轻量托管',
        tone: 'brand',
    },
    {
        value: 'snowluma',
        label: 'SnowLuma',
        badge: '带 QQ GUI',
        summary: '保留 QQ 客户端窗口，支持桌面交互与远端 VNC',
        tone: 'info',
    },
];

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

export function IdentityTab({
    data,
    onChange,
    isEditMode,
    isRunning,
    containerEngine,
    onContainerEngineChange,
    containerEngineLocked = false,
}: IdentityTabProps) {
    const { servers, isLoading: serversLoading } = useServerManager();

    // 设置里关掉的协议端不给选；已经是它的（老配置）照常显示
    const features = useFeatures();
    const backendItems = useMemo(
        () => BACKEND_ITEMS.filter((i) => features[i.value] || i.value === data.backend_type),
        [features, data.backend_type],
    );
    // 新建时默认的底座被关了，换成开着的那个（两个至少留一个）
    useEffect(() => {
        if (isEditMode || features[data.backend_type]) return;
        onChange({ backend_type: features.napcat ? 'napcat' : 'snowluma' });
        // eslint-disable-next-line react-hooks/exhaustive-deps -- onChange 每次渲染都是新的
    }, [isEditMode, features, data.backend_type]);

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
    const remoteReadiness = useRuntimeReadiness(
        isRemote && deploymentType === 'native' ? remoteHostId : null,
        data.backend_type,
    );
    const localReadiness = useRuntimeReadiness(
        !isRemote && deploymentType === 'native' ? 'local' : null,
        data.backend_type,
    );
    const componentNames = useComponentNames();

    const remoteReachable = useIsHostReachable(remoteHostId);
    const remoteTransportFailed = isRemote && remoteHostId != null && !remoteReachable;

    const missingDirectRunNotice = useMemo(() => {
        if (!isRemote || deploymentType !== 'native' || !remoteHostId) {
            return null;
        }
        if (!remoteReadiness.readiness) return null;
        const blocking = describeBlocking(remoteReadiness.readiness, componentNames);
        return blocking ? `远程主机${blocking}，请到「组件」页安装` : null;
    }, [isRemote, deploymentType, remoteHostId, remoteReadiness.readiness, componentNames]);

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

    const namePlaceholder = useMemo(() => {
        if (data.QQID > 0) {
            const tail = String(data.QQID).slice(-4);
            return `Bot-${tail}`;
        }
        return '例如：Bot-01';
    }, [data.QQID]);

    const patchSchedule = (patch: Partial<AutoRestartSchedule>) =>
        onChange({ autoRestartSchedule: { ...data.autoRestartSchedule, ...patch } });

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
    const localBlocking = localReadiness.readiness
        ? describeBlocking(localReadiness.readiness, componentNames)
        : null;
    const localNotice = localBlocking
        ? `本机${localBlocking}，请到「组件」页安装后再使用本机直接运行`
        : localReadiness.probing && !localReadiness.readiness
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
        <div className="flex w-full flex-col gap-6">
            <div data-tour-id="bot-identity-section">
                <FormSection title="账号与底座">
                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                        <NumberField
                            label="QQ 账号"
                            required
                            value={data.QQID || null}
                            onValueChange={(v) => onChange({ QQID: v ?? 0 })}
                            placeholder="例如：10001"
                            disabled={isEditMode}
                        />
                        <TextField
                            label="实例名称"
                            value={data.name}
                            onValueChange={(v) => onChange({ name: v })}
                            placeholder={namePlaceholder}
                        />
                    </div>

                    <div className="flex flex-col gap-1.5">
                        <div className="flex items-center justify-between gap-2">
                            <span className="text-xs font-medium text-text-secondary">
                                底座类型
                            </span>
                            {isRunning && (
                                <span className="text-2xs text-text-tertiary">
                                    运行中需先停止再切换底座
                                </span>
                            )}
                        </div>
                        <div
                            role="radiogroup"
                            aria-label="底座类型"
                            className="grid grid-cols-1 gap-3 sm:grid-cols-2"
                        >
                            {backendItems.map((item) => {
                                const selected = data.backend_type === item.value;
                                return (
                                    <button
                                        key={item.value}
                                        type="button"
                                        role="radio"
                                        aria-checked={selected}
                                        disabled={isRunning}
                                        onClick={() => onChange({ backend_type: item.value })}
                                        className={cn(
                                            'group relative flex h-full min-w-0 flex-col justify-between gap-1 rounded-md border px-3.5 py-2.5 text-left transition-all',
                                            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand',
                                            'disabled:cursor-not-allowed disabled:opacity-60',
                                            selected
                                                ? 'border-brand/70 bg-brand-soft/20 shadow-xs ring-1 ring-brand/25'
                                                : 'border-border-subtle bg-field/45 hover:border-border hover:bg-field/80',
                                        )}
                                    >
                                        <div className="flex items-center justify-between gap-2">
                                            <div className="flex min-w-0 items-center gap-2">
                                                <span className="truncate text-sm font-semibold text-text">
                                                    {item.label}
                                                </span>
                                                <Badge
                                                    tone={item.tone}
                                                    appearance="soft"
                                                    className="shrink-0 text-[11px]"
                                                >
                                                    {item.badge}
                                                </Badge>
                                            </div>
                                            <SelectionDot selected={selected} />
                                        </div>
                                        <p className="text-2xs leading-relaxed text-text-tertiary">
                                            {item.summary}
                                        </p>
                                    </button>
                                );
                            })}
                        </div>
                    </div>
                </FormSection>
            </div>

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
                                                            item.value === 'docker' &&
                                                            !dockerAllowed
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
                                                dockerHostId
                                                    ? statusByHost[dockerProbeKey]
                                                    : undefined
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
                                        deploymentType === 'native' &&
                                        !!remoteHostId &&
                                        !!remoteNotice
                                    }
                                >
                                    <div className="pt-3">
                                        <InlineNotice
                                            tone={remoteTransportFailed ? 'danger' : 'warn'}
                                        >
                                            {remoteNotice}
                                        </InlineNotice>
                                    </div>
                                </ExpandPresence>
                            </div>
                        </div>
                    </ExpandPresence>

                    <ExpandPresence
                        visible={!isRemote && deploymentType === 'native' && !!localNotice}
                    >
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

            <FormSection title="自愈与附加服务" layout="none">
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                        <div className="w-full">
                            <Switch
                                label="掉线自动重启"
                                hint="进程异常退出或掉线时自动拉起"
                                checked={data.offlineAutoRestart}
                                onCheckedChange={(v) => onChange({ offlineAutoRestart: v })}
                            />
                        </div>
                    </div>
                    <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                        <div className="w-full">
                            <Switch
                                label="定时自动重启"
                                hint="按固定间隔或 Cron 计划重启实例"
                                checked={data.autoRestartSchedule.enable}
                                onCheckedChange={(v) => patchSchedule({ enable: v })}
                            />
                        </div>
                    </div>
                </div>
                <ExpandPresence visible={data.autoRestartSchedule.enable}>
                    <div className="pt-3">
                        <div className="rounded-md border border-border-subtle/70 bg-field/30 px-3.5 py-3">
                            <AutoRestartScheduleEditor
                                schedule={data.autoRestartSchedule}
                                onChange={patchSchedule}
                            />
                        </div>
                    </div>
                </ExpandPresence>
                <div className="pt-3">
                    <div className="grid grid-cols-1 items-center gap-3 rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5 sm:grid-cols-2">
                        <div className="min-w-0">
                            <div className="text-sm font-medium text-text">音乐签名接口</div>
                            <div className="text-2xs text-text-tertiary">
                                仅发送音乐卡片时需要，留空则不启用
                            </div>
                        </div>
                        <TextField
                            aria-label="音乐签名接口"
                            value={data.musicSignUrl}
                            onValueChange={(v) => onChange({ musicSignUrl: v })}
                            placeholder="http://127.0.0.1:8081/sign"
                            className="font-mono"
                        />
                    </div>
                </div>
            </FormSection>
        </div>
    );
}

function SelectionDot({ selected }: { selected: boolean }) {
    return (
        <span
            aria-hidden
            className={cn(
                'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full border transition-colors',
                selected ? 'border-brand bg-brand/10' : 'border-border-strong bg-field',
            )}
        >
            <span
                className={cn(
                    'h-2 w-2 rounded-full bg-brand transition-transform duration-150',
                    selected ? 'scale-100 opacity-100' : 'scale-50 opacity-0',
                )}
            />
        </span>
    );
}

function InlineNotice({
    tone,
    children,
}: {
    tone: 'ok' | 'warn' | 'neutral' | 'danger';
    children: ReactNode;
}) {
    const cls =
        tone === 'ok'
            ? 'border-success/25 bg-success-soft/60 text-success'
            : tone === 'danger'
              ? 'border-danger/25 bg-danger-soft/60 text-danger'
              : tone === 'warn'
                ? 'border-warning/25 bg-warning-soft/60 text-warning'
                : 'border-border-subtle bg-inset text-text-secondary';
    const Icon =
        tone === 'ok' ? CheckCircle2 : tone === 'danger' || tone === 'warn' ? AlertTriangle : Info;
    return (
        <div
            className={cn(
                'flex items-start gap-2 rounded-sm border px-3 py-2 text-2xs leading-relaxed',
                cls,
            )}
        >
            <Icon size={13} className="mt-0.5 shrink-0" />
            <div className="min-w-0 flex-1">{children}</div>
        </div>
    );
}

function SceneExplainerPopover({ children }: { children: ReactNode }) {
    return (
        <Popover>
            <PopoverTrigger asChild>
                <button
                    type="button"
                    className="inline-flex h-6 items-center gap-1 rounded-sm px-1.5 text-[11px] text-text-tertiary transition-colors hover:bg-inset hover:text-text"
                >
                    <CircleHelp size={12} />
                    <span>机制说明</span>
                </button>
            </PopoverTrigger>
            <PopoverContent side="bottom" align="end" sideOffset={6}>
                <p className="max-w-xs text-[12px] leading-relaxed text-text-secondary">
                    {children}
                </p>
            </PopoverContent>
        </Popover>
    );
}

function DockerReadinessLine({
    engine,
    flavorLabel,
    status,
    probing,
    imageReady,
}: {
    flavorLabel: string;
    engine?: ContainerEngine;
    status: import('../../../../core/ipc/types').DockerStatus | undefined;
    probing: boolean;
    imageReady: boolean | undefined;
}) {
    const notice = dockerReadinessNotice({
        engine,
        flavorLabel,
        status,
        probing,
        imageReady,
    });
    if (!notice) return null;
    return <InlineNotice tone={notice.tone}>{notice.text}</InlineNotice>;
}

const SL_MODE_ITEMS: ReadonlyArray<{
    value: 'cold_start' | 'hot_start';
    label: string;
    summary: string;
    icon: ComponentType<LucideProps>;
}> = [
    {
        value: 'cold_start',
        label: '冷启动（默认）',
        summary: '由桌面端拉起并托管 QQ 进程',
        icon: Snowflake,
    },
    {
        value: 'hot_start',
        label: '热启动',
        summary: '附加到已登录此号的 QQ 进程',
        icon: Flame,
    },
];

function SnowLumaStartModeBlock({
    data,
    onChange,
}: {
    data: BotBasicConfig;
    onChange: (patch: Partial<BotBasicConfig>) => void;
}) {
    const mode: 'cold_start' | 'hot_start' =
        data.snowlumaStartMode?.mode === 'hot_start' ? 'hot_start' : 'cold_start';

    const setMode = (m: 'cold_start' | 'hot_start') => {
        const value: SnowLumaStartMode =
            m === 'cold_start' ? { mode: 'cold_start' } : { mode: 'hot_start' };
        onChange({ snowlumaStartMode: value });
    };

    return (
        <div className="flex flex-col">
            <div
                role="radiogroup"
                aria-label="SnowLuma 启动模式"
                className="grid grid-cols-1 gap-3 sm:grid-cols-2"
            >
                {SL_MODE_ITEMS.map((item) => {
                    const selected = mode === item.value;
                    const Icon = item.icon;
                    return (
                        <button
                            key={item.value}
                            type="button"
                            role="radio"
                            aria-checked={selected}
                            onClick={() => setMode(item.value)}
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
            <ExpandPresence visible={mode === 'hot_start'}>
                <div className="pt-2.5">
                    <InlineNotice tone="neutral">
                        启动前需在此 QQ 号（
                        <span className="font-mono">{data.QQID || '未填'}</span>
                        ）下登录 QQ；找不到匹配进程时启动会失败
                    </InlineNotice>
                </div>
            </ExpandPresence>
        </div>
    );
}
