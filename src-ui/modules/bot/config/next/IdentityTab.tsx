// 身份 Tab：QQ 账号 + 实例名 + 底座选择 + SnowLuma 启动模式 + 运行宿主 + 自愈策略。

import { useMemo, type ReactNode } from 'react';
import {
    TextField,
    NumberField,
    Select,
    Switch,
    RadioGroup,
    FormSection,
} from '../../../../shared/ui';
import { useServerManager } from '../../../../hooks/remote/useServerManager';
import { useDockerHosts } from '../../../../hooks/docker/useDockerHosts';
import { useHostComponentInstalled } from '../../../../hooks/components/useRemoteHostComponentInstalled';
import {
    remoteDirectRunChain,
    localDirectRunChain,
    componentIdToDisplayName,
    formatMissingDirectRunNotice,
} from '../../../../core/domain/bot/remote-direct-run-deps';
import { isRuntimeTargetConcreteRemote } from '../../../../core/domain/bot/runtime-target';
import { dockerReadinessNotice } from '../../../../core/domain/bot/docker-start-gate';
import {
    RUNTIME_TARGET_REMOTE_PLACEHOLDER,
    isRuntimeTargetLocal,
    runtimeModeForTarget,
    remoteHostIdFromRuntimeTarget,
    serverProfileIdFromRuntimeTarget,
} from '../../../../core/domain/bot/runtime-target';
import type { BotBasicConfig } from '../../../../core/ipc/generated/domain/BotBasicConfig';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
import type { DeploymentType } from '../../../../core/ipc/generated/domain/DeploymentType';
import type { TimeUnit } from '../../../../core/ipc/generated/domain/TimeUnit';
import type { SnowLumaStartMode } from '../../../../core/ipc/generated/domain/SnowLumaStartMode';

interface IdentityTabProps {
    data: BotBasicConfig;
    onChange: (patch: Partial<BotBasicConfig>) => void;
    isEditMode: boolean;
    isRunning: boolean;
}

const BACKEND_ITEMS = [
    { value: 'napcat' as BackendType, label: 'NapCat（不带 QQ GUI）' },
    { value: 'snowluma' as BackendType, label: 'SnowLuma（带 QQ GUI）' },
];

const RUNTIME_ITEMS = [
    { value: 'local', label: '本机' },
    { value: 'remote', label: '远程主机' },
];

const DEPLOYMENT_ITEMS = [
    { value: 'native' as DeploymentType, label: '直接运行' },
    { value: 'docker' as DeploymentType, label: 'Docker' },
];

const TIME_UNIT_ITEMS = [
    { value: 'm' as TimeUnit, label: '分钟' },
    { value: 'h' as TimeUnit, label: '小时' },
    { value: 'd' as TimeUnit, label: '天' },
    { value: 'mon' as TimeUnit, label: '月' },
    { value: 'year' as TimeUnit, label: '年' },
];

export function IdentityTab({ data, onChange, isEditMode, isRunning }: IdentityTabProps) {
    const { servers, isLoading: serversLoading } = useServerManager();

    const isRemote = !isRuntimeTargetLocal(data.runtime_target);
    const runtimeMode = runtimeModeForTarget(data.runtime_target);

    const serverItems = useMemo(
        () => servers.map((s) => ({ value: s.id, label: `${s.name} · ${s.host}` })),
        [servers],
    );

    const hasRemoteHosts = servers.length > 0;

    const deploymentType: DeploymentType =
        data.deploymentType === 'docker' ? 'docker' : 'native';

    const remoteHostId = useMemo(
        () =>
            isRemote
                ? remoteHostIdFromRuntimeTarget(data.runtime_target)
                : null,
        [isRemote, data.runtime_target],
    );

    const dockerHostIds = useMemo(
        () => (remoteHostId ? [remoteHostId] : []),
        [remoteHostId],
    );
    const { statusByHost, probingByHost, imageReadyByHost } =
        useDockerHosts(dockerHostIds);

    const componentInstalled = useHostComponentInstalled(
        remoteHostId,
        data.backend_type,
    );

    const localInstalled = useHostComponentInstalled('local', data.backend_type);

    const missingDirectRunNotice = useMemo(() => {
        if (!isRemote || deploymentType !== 'native' || !remoteHostId) {
            return null;
        }
        const chain = remoteDirectRunChain(data.backend_type);
        const missing: string[] = [];
        for (const id of chain) {
            if (componentInstalled[id] === false) {
                missing.push(componentIdToDisplayName(id));
            }
        }
        if (missing.length === 0) return null;
        return `未安装 ${missing.join('、')}，请安装`;
    }, [
        isRemote,
        deploymentType,
        remoteHostId,
        data.backend_type,
        componentInstalled,
    ]);

    const onRuntimeModeChange = (mode: string) => {
        if (mode === 'local') {
            onChange({ runtime_target: 'local', deploymentType: 'native' });
        } else {
            const only =
                servers.length === 1
                    ? servers[0].id
                    : RUNTIME_TARGET_REMOTE_PLACEHOLDER;
            onChange({ runtime_target: only });
        }
    };

    const namePlaceholder = useMemo(() => {
        if (data.QQID > 0) {
            const tail = String(data.QQID).slice(-4);
            return `Bot-${tail}`;
        }
        return '例如：Bot-01';
    }, [data.QQID]);

    const remoteHostSelectValue: string | undefined = (() => {
        const profileId = serverProfileIdFromRuntimeTarget(data.runtime_target);
        if (!profileId) return undefined;
        if (servers.some((s) => s.id === profileId)) return profileId;
        return undefined;
    })();

    const showDockerBlock =
        isRemote &&
        deploymentType === 'docker' &&
        (data.backend_type === 'napcat' || data.backend_type === 'snowluma') &&
        remoteHostId != null;

    const dockerFlavorLabel =
        data.backend_type === 'snowluma' ? 'SnowLuma' : 'NapCat';

    return (
        <div className="flex flex-col gap-8">
            <FormSection
                title="账号身份"
                description="QQ 账号、实例显示名与底座类型"
            >
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    <NumberField
                        label="QQ 账号"
                        required
                        value={data.QQID || null}
                        onValueChange={(v) => onChange({ QQID: v ?? 0 })}
                        placeholder="例如：10001"
                        disabled={isEditMode}
                        hint={isEditMode ? '编辑模式下不可修改' : undefined}
                    />
                    <TextField
                        label="实例名称"
                        value={data.name}
                        onValueChange={(v) => onChange({ name: v })}
                        placeholder={namePlaceholder}
                    />
                </div>
                <Select
                    label="底座类型"
                    items={BACKEND_ITEMS}
                    value={data.backend_type}
                    onValueChange={(v) => onChange({ backend_type: v })}
                    disabled={isRunning}
                    hint={
                        isRunning
                            ? '运行中请先停止再切换底座'
                            : undefined
                    }
                />
            </FormSection>

            {data.backend_type === 'napcat' &&
                deploymentType === 'native' &&
                isRemote &&
                isRuntimeTargetConcreteRemote(data.runtime_target) && (
                    <FormSection
                        title="NapCat 远端直接运行"
                        description="WebUI 在 SSH 主机本机 6099；桌面经隧道打开并轮询登录态"
                        layout="none"
                    >
                        <InlineNotice tone="neutral">
                            Bot 启动后桌面会建立 SSH 转发并解析远端 NapCat 日志中的 WebUI
                            地址；列表 WebUI 可用后可在浏览器打开（带 token）。日志尚未打出 WebUI
                            行时，会暂用与 Docker 相同的持久化 token 尝试登录轮询。
                        </InlineNotice>
                    </FormSection>
                )}

            {data.backend_type === 'snowluma' &&
                deploymentType === 'native' &&
                isRemote &&
                isRuntimeTargetConcreteRemote(data.runtime_target) && (
                    <FormSection
                        title="SnowLuma 远端直接运行"
                        description="图形栈在 SSH 主机上；桌面端经隧道打开 WebUI 与 noVNC"
                        layout="none"
                    >
                        <InlineNotice tone="neutral">
                            启动后桌面会把远端 5099 / 6081 转发到本机回环地址，列表可打开 WebUI
                            与 noVNC 扫码。登录密码来自远端 secret 文件，打开时会复制到剪贴板。冷/热启动
                            仍由下方「启动模式」决定。
                        </InlineNotice>
                    </FormSection>
                )}

            {data.backend_type === 'snowluma' && deploymentType !== 'docker' && (
                <FormSection
                    title="SnowLuma 启动模式"
                    description="冷启动由桌面端拉起 QQ；热启动附加到已登录此号的 QQ 进程"
                >
                    <SnowLumaStartModeBlock data={data} onChange={onChange} />
                </FormSection>
            )}

            {data.backend_type === 'snowluma' && deploymentType === 'docker' && (
                <FormSection
                    title="SnowLuma Docker"
                    description="容器内自带 QQ 图形环境，扫码请用远程主机 noVNC（默认端口 6081）"
                    layout="none"
                >
                    <InlineNotice tone="neutral">
                        Docker 模式下不使用本机冷/热启动；登录在容器内完成。VNC 与 SnowLuma
                        WebUI 密码由桌面端生成并写入 compose（分别对应 noVNC 与 5099 登录）。
                        列表卡片在隧道就绪后可打开 noVNC 扫码页与 WebUI。
                    </InlineNotice>
                </FormSection>
            )}

            <FormSection
                title="运行宿主"
                description="仅远程 Linux 支持 Docker；本机固定为直接运行"
                layout="none"
            >
                <div className="flex flex-col gap-4">
                    <RadioGroup
                        key={`runtime-mode-${runtimeMode}`}
                        items={RUNTIME_ITEMS}
                        value={runtimeMode}
                        onValueChange={onRuntimeModeChange}
                        orientation="horizontal"
                        name="runtime-target"
                    />

                    {isRemote && (
                        <div className="flex flex-col gap-3 sm:max-w-md">
                            {!hasRemoteHosts && !serversLoading && (
                                <InlineNotice tone="warn">
                                    请先在「远程主机」页添加 SSH 主机；当前配置仍按远程保存
                                </InlineNotice>
                            )}
                            {hasRemoteHosts && serverItems.length > 0 ? (
                                <Select
                                    label="远程主机"
                                    items={serverItems}
                                    value={remoteHostSelectValue}
                                    onValueChange={(v) =>
                                        onChange({ runtime_target: v })
                                    }
                                    placeholder="选择主机"
                                />
                            ) : isRemote && hasRemoteHosts ? (
                                <InlineNotice tone="warn">
                                    请先在「远程主机」页添加 SSH 主机
                                </InlineNotice>
                            ) : null}

                                <div className="space-y-2">
                                    <span className="text-xs font-medium text-text-secondary">
                                        启动方式
                                    </span>
                                    <RadioGroup
                                        items={DEPLOYMENT_ITEMS}
                                        value={deploymentType}
                                        onValueChange={(v) =>
                                            onChange({
                                                deploymentType:
                                                    v as DeploymentType,
                                            })
                                        }
                                        orientation="horizontal"
                                        name="deployment-type"
                                    />
                                </div>

                                {showDockerBlock && remoteHostId && (
                                    <DockerReadinessLine
                                        flavorLabel={dockerFlavorLabel}
                                        status={statusByHost[remoteHostId]}
                                        probing={
                                            probingByHost[remoteHostId] ?? false
                                        }
                                        imageReady={
                                            imageReadyByHost[remoteHostId]?.[
                                                data.backend_type === 'snowluma'
                                                    ? 'snowluma'
                                                    : 'napcat'
                                            ]
                                        }
                                    />
                                )}

                                {isRemote && deploymentType === 'native' &&
                                    missingDirectRunNotice && (
                                        <InlineNotice tone="warn">
                                            {missingDirectRunNotice}
                                        </InlineNotice>
                                    )}

                                {/* 本地直接运行提示 */}
                                {!isRemote && (
                                    (() => {
                                        const chain = localDirectRunChain(data.backend_type);
                                        const missing = chain.filter(
                                            (id) => localInstalled[id] === false,
                                        );
                                        if (missing.length > 0) {
                                            return (
                                                <InlineNotice tone="warn">
                                                    本机缺少 {missing.map(componentIdToDisplayName).join('、')}，请到「组件」页安装后再使用本机直接运行
                                                </InlineNotice>
                                            );
                                        }
                                        if (chain.some((id) => localInstalled[id] === undefined)) {
                                            return <InlineNotice tone="neutral">正在检测本机运行时组件...</InlineNotice>;
                                        }
                                        return null;
                                    })()
                                )}
                            </div>
                        )}
                    </div>
                </FormSection>

            <FormSection
                title="附加服务"
                description="非必填，仅在需要发送音乐卡片时配置"
            >
                <TextField
                    label="音乐签名接口"
                    value={data.musicSignUrl}
                    onValueChange={(v) => onChange({ musicSignUrl: v })}
                    placeholder="http://127.0.0.1:8081/sign"
                />
            </FormSection>

            <FormSection
                title="自愈与定时重启"
                description="掉线自动恢复与周期重启"
            >
                <Switch
                    label="掉线自动重启"
                    checked={data.offlineAutoRestart}
                    onCheckedChange={(v) => onChange({ offlineAutoRestart: v })}
                />
                <Switch
                    label="定时自动重启"
                    checked={data.autoRestartSchedule.enable}
                    onCheckedChange={(v) =>
                        onChange({
                            autoRestartSchedule: {
                                ...data.autoRestartSchedule,
                                enable: v,
                            },
                        })
                    }
                />
                {data.autoRestartSchedule.enable && (
                    <div className="ml-7 grid grid-cols-1 gap-3 sm:grid-cols-2">
                        <NumberField
                            label="重启间隔"
                            value={data.autoRestartSchedule.duration}
                            onValueChange={(v) =>
                                onChange({
                                    autoRestartSchedule: {
                                        ...data.autoRestartSchedule,
                                        duration: v ?? 0,
                                    },
                                })
                            }
                            min={1}
                        />
                        <Select
                            label="时间单位"
                            items={TIME_UNIT_ITEMS}
                            value={data.autoRestartSchedule.time_unit}
                            onValueChange={(v) =>
                                onChange({
                                    autoRestartSchedule: {
                                        ...data.autoRestartSchedule,
                                        time_unit: v,
                                    },
                                })
                            }
                        />
                    </div>
                )}
            </FormSection>
        </div>
    );
}

function InlineNotice({
    tone,
    children,
}: {
    tone: 'ok' | 'warn' | 'neutral';
    children: ReactNode;
}) {
    const cls =
        tone === 'ok'
            ? 'bg-success-soft text-success'
            : tone === 'warn'
              ? 'bg-warning-soft text-warning'
              : 'bg-inset text-text-tertiary';
    return (
        <p className={`rounded-sm px-3 py-2 text-2xs leading-relaxed ${cls}`}>
            {children}
        </p>
    );
}

function DockerReadinessLine({
    flavorLabel,
    status,
    probing,
    imageReady,
}: {
    flavorLabel: string;
    status: import('../../../../core/ipc/types').DockerStatus | undefined;
    probing: boolean;
    imageReady: boolean | undefined;
}) {
    const notice = dockerReadinessNotice({
        flavorLabel,
        status,
        probing,
        imageReady,
    });
    if (!notice) return null;
    return <InlineNotice tone={notice.tone}>{notice.text}</InlineNotice>;
}

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
        <>
            <RadioGroup
                items={[
                    { value: 'cold_start', label: '冷启动（推荐）' },
                    { value: 'hot_start', label: '热启动' },
                ]}
                value={mode}
                onValueChange={setMode}
                name="snowluma-mode"
            />
            {mode === 'hot_start' && (
                <InlineNotice tone="neutral">
                    启动前请在此 QQ 号（
                    <span className="font-mono">{data.QQID || '未填'}</span>
                    ）下登录 QQ；找不到匹配进程时启动会失败
                </InlineNotice>
            )}
        </>
    );
}