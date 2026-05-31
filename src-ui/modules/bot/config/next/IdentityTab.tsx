// 身份 Tab：QQ 账号 + 实例名 + 底座选择 + SnowLuma 启动模式 + 运行宿主 + 自愈策略。
//
// vs 旧版 BotBasicTab 的关键差异：
//   - QQ ID + 实例名一行并排（grid-2），实例名 placeholder 自动跟随 QQID
//   - 底座 NapCat / SnowLuma 用 Select；Bot 处于 Running / Starting 时禁用
//     切换（运行中改 backend 没有有效语义，等同重新部署）
//   - SnowLuma 只暴露 cold / hot 两选项；HotStart 不再让用户选 PID，
//     backend 启动时按 qq_id 自动扫一遍登录状态匹配
//   - 运行宿主 RadioGroup 横排（仅 2 项，没必要立式占 2 行）
//   - autoRestart duration / unit 一行并排

import { useMemo } from 'react';
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
import type { BotBasicConfig } from '../../../../core/ipc/generated/domain/BotBasicConfig';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
import type { DeploymentType } from '../../../../core/ipc/generated/domain/DeploymentType';
import type { TimeUnit } from '../../../../core/ipc/generated/domain/TimeUnit';
import type { SnowLumaStartMode } from '../../../../core/ipc/generated/domain/SnowLumaStartMode';

interface IdentityTabProps {
    data: BotBasicConfig;
    onChange: (patch: Partial<BotBasicConfig>) => void;
    /** 编辑模式下 QQ ID 不可改。 */
    isEditMode: boolean;
    /**
     * Bot 当前是否在运行（Running / Starting / Stopping）。
     * 运行中禁止切换底座类型——这等同于"重新部署"，必须先停止再切。
     */
    isRunning: boolean;
}

const BACKEND_ITEMS = [
    { value: 'napcat' as BackendType, label: 'NapCat（带 QQ GUI）' },
    { value: 'snowluma' as BackendType, label: 'SnowLuma（不带 QQ GUI）' },
];

const RUNTIME_ITEMS = [
    { value: 'local', label: '本机', hint: '在当前电脑上启动' },
    { value: 'remote', label: '远程 SSH 主机', hint: '通过 SSH 在远端启动' },
];

const DEPLOYMENT_ITEMS = [
    { value: 'native' as DeploymentType, label: '直接运行', hint: '在主机上直接拉起进程（默认）' },
    { value: 'docker' as DeploymentType, label: 'Docker 容器', hint: '用 docker compose 起容器，仅 NapCat 底座' },
];

const TIME_UNIT_ITEMS = [
    { value: 'm' as TimeUnit, label: '分钟' },
    { value: 'h' as TimeUnit, label: '小时' },
    { value: 'd' as TimeUnit, label: '天' },
    { value: 'mon' as TimeUnit, label: '月' },
    { value: 'year' as TimeUnit, label: '年' },
];

export function IdentityTab({ data, onChange, isEditMode, isRunning }: IdentityTabProps) {
    // 远程主机列表（用于"远程"模式下选具体机器）。
    const { servers } = useServerManager();

    // runtime_target 语义:'local' = 本机;其它字符串 = 具体远程 server_id;
    // 'remote' 是占位(选了远程但还没选机器),保存时被 validate 挡住。
    const isRemote = data.runtime_target !== 'local';
    // 给"本机/远程"RadioGroup 用的值:本机=local,远程=remote(占位或已选机器都算远程)。
    const runtimeMode = data.runtime_target === 'local' ? 'local' : 'remote';

    const serverItems = useMemo(
        () => servers.map((s) => ({ value: s.id, label: `${s.name}（${s.host}）` })),
        [servers],
    );

    const onRuntimeModeChange = (mode: string) => {
        if (mode === 'local') {
            onChange({ runtime_target: 'local' });
        } else {
            // 切到远程:只有一台就直接选它,多台/没有给占位 'remote' 让用户在下拉里选。
            const only = servers.length === 1 ? servers[0].id : 'remote';
            onChange({ runtime_target: only });
        }
    };

    // 实例名占位：QQID 改了就更新，避免空白让用户面对"不知道写啥"的反应
    const namePlaceholder = useMemo(() => {
        if (data.QQID > 0) {
            const tail = String(data.QQID).slice(-4);
            return `Bot-${tail}`;
        }
        return '例如：Bot-01';
    }, [data.QQID]);

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
                        hint={isEditMode ? '编辑模式下不可修改 QQ 账号' : '托管目标的纯数字 QQ 号'}
                    />
                    <TextField
                        label="实例名称"
                        value={data.name}
                        onValueChange={(v) => onChange({ name: v })}
                        placeholder={namePlaceholder}
                        hint="用于在控制台中识别此 Bot"
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
                            ? 'Bot 运行中无法切换底座，请先停止此实例再修改'
                            : '切换底座会改变可用的连接类型与高级特性'
                    }
                />
            </FormSection>

            {data.backend_type === 'snowluma' && (
                <FormSection
                    title="SnowLuma 启动模式"
                    description="决定 SnowLuma 是自己启动 QQ.exe，还是附加到一个已登录此账号的 QQ 进程"
                >
                    <SnowLumaStartModeBlock data={data} onChange={onChange} />
                </FormSection>
            )}

            <FormSection
                title="运行宿主"
                description="Bot 引擎实际跑在哪台机器上、以什么方式启动"
            >
                <RadioGroup
                    items={RUNTIME_ITEMS}
                    value={runtimeMode}
                    onValueChange={onRuntimeModeChange}
                    orientation="horizontal"
                    name="runtime-target"
                />
                {isRemote && (
                    serverItems.length > 0 ? (
                        <Select
                            label="选择远程主机"
                            items={serverItems}
                            value={data.runtime_target === 'remote' ? '' : data.runtime_target}
                            onValueChange={(v) => onChange({ runtime_target: v })}
                            placeholder="请选择一台已添加的远程主机"
                            hint="在远端 SSH 主机上启动；主机在「远程主机」页添加"
                        />
                    ) : (
                        <p className="rounded-sm bg-warning-soft px-3 py-2 text-2xs leading-relaxed text-warning">
                            还没有可用的远程主机。请先到「远程主机」页添加并连接一台 SSH 主机。
                        </p>
                    )
                )}

                <RadioGroup
                    items={DEPLOYMENT_ITEMS}
                    value={data.deploymentType}
                    onValueChange={(v) => onChange({ deploymentType: v as DeploymentType })}
                    orientation="horizontal"
                    name="deployment-type"
                />
                {data.deploymentType === 'docker' && data.backend_type !== 'napcat' && (
                    <p className="rounded-sm bg-warning-soft px-3 py-2 text-2xs leading-relaxed text-warning">
                        Docker 启动方式当前仅支持 NapCat 底座。SnowLuma 容器化待后续支持，请改回「直接运行」。
                    </p>
                )}
            </FormSection>

            <RuntimeDependencyHint
                runtimeTarget={data.runtime_target}
                deploymentType={data.deploymentType}
                backendType={data.backend_type}
            />

            <FormSection
                title="附加服务"
                description="非必填，仅在需要发送音乐卡片时配置"
            >
                <TextField
                    label="音乐签名接口"
                    value={data.musicSignUrl}
                    onValueChange={(v) => onChange({ musicSignUrl: v })}
                    placeholder="http://127.0.0.1:8081/sign"
                    hint="发送网易云 / QQ 音乐卡片时所需的签名服务器，留空跳过"
                />
            </FormSection>


            <FormSection
                title="自愈与定时重启"
                description="掉线自动恢复，避免长时间无人值守时静默故障"
            >
                <Switch
                    label="掉线自动重启"
                    hint="检测到 Bot 离线时尝试自动拉起"
                    checked={data.offlineAutoRestart}
                    onCheckedChange={(v) => onChange({ offlineAutoRestart: v })}
                />
                <Switch
                    label="定时自动重启"
                    hint="按固定周期重启 Bot，定期清理累积状态"
                    checked={data.autoRestartSchedule.enable}
                    onCheckedChange={(v) =>
                        onChange({
                            autoRestartSchedule: { ...data.autoRestartSchedule, enable: v },
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

// ────────────────────────────────────────────────────────────────────
// 运行时依赖检查 + 引导安装。
// 选定运行宿主 + 启动方式后,提示这台机器是否就绪:
//   - Docker 启动:检查该 host 的 docker 是否就绪,没就绪给「去组件页装」入口。
//   - 直接运行:提示去组件页确认 NodeJs / NapCat 等运行时已装。
// 探测复用 useDockerHosts(docker 状态),不在这层重造检测逻辑。
// ────────────────────────────────────────────────────────────────────

function RuntimeDependencyHint({
    runtimeTarget,
    deploymentType,
    backendType,
}: {
    runtimeTarget: string;
    deploymentType: DeploymentType;
    backendType: BackendType;
}) {
    // runtime_target -> host_id:local 直接用,'remote' 占位时还没选机器先不探,
    // 具体 server_id 拼成 remote:<id>。
    const hostId =
        runtimeTarget === 'local'
            ? 'local'
            : runtimeTarget === 'remote'
                ? null
                : `remote:${runtimeTarget}`;

    const hostIds = useMemo(() => (hostId ? [hostId] : []), [hostId]);
    const { statusByHost, probingByHost } = useDockerHosts(hostIds);

    // 还没选具体远程机器:不显示依赖块(上面已经有"请选主机"提示)。
    if (!hostId) return null;

    const isDocker = deploymentType === 'docker';

    if (isDocker) {
        const status = statusByHost[hostId];
        const probing = probingByHost[hostId] ?? false;
        const ready = status?.installed && status?.daemonRunning && status?.composeAvailable;
        if (probing && !status) {
            return <DepBox tone="neutral" text="正在检查这台机器的 Docker 状态…" />;
        }
        if (ready) {
            return <DepBox tone="ok" text={`Docker ${status?.version ?? ''} 已就绪，可以用容器方式启动。`} />;
        }
        return (
            <DepBox
                tone="warn"
                text="这台机器的 Docker 尚未就绪（未安装 / 守护进程未运行 / 缺 compose）。请到「组件」页选这台机器安装 Docker 后再启动。"
            />
        );
    }

    // 直接运行:运行时(NodeJs/NapCat)的安装状态在组件页管理,这里给一句引导。
    const name = backendType === 'snowluma' ? 'SnowLuma' : 'NapCat';
    return (
        <DepBox
            tone="neutral"
            text={`直接运行需要这台机器已安装 ${name} 运行时依赖（NodeJs / ${name} 等）。可到「组件」页选这台机器确认并安装。`}
        />
    );
}

function DepBox({ tone, text }: { tone: 'ok' | 'warn' | 'neutral'; text: string }) {
    const cls =
        tone === 'ok'
            ? 'bg-success-soft text-success'
            : tone === 'warn'
                ? 'bg-warning-soft text-warning'
                : 'bg-inset text-text-tertiary';
    return (
        <div className={`rounded-sm px-3 py-2 text-2xs leading-relaxed ${cls}`}>{text}</div>
    );
}

// ────────────────────────────────────────────────────────────────────
// SnowLuma 启动模式块。COLD / HOT 两选项；HotStart 不再让用户选 PID，
// backend 启动时按 qq_id 自动扫描登录状态匹配。这一层 UI 只决定一个 enum
// variant，落盘后由 BotConfigPage 顶层统一保存。
// ────────────────────────────────────────────────────────────────────

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
                    {
                        value: 'cold_start',
                        label: 'COLD - 自动启动新的 QQ.exe（推荐）',
                        hint: '由 NapCatQQ-Desktop 全权管理 QQ 进程的生命周期',
                    },
                    {
                        value: 'hot_start',
                        label: 'HOT - 附加到已登录此账号的 QQ.exe',
                        hint: '保留你已经手动登录的会话，启动时按 QQ 号自动定位 PID 注入；stop 时不会杀掉这个 QQ',
                    },
                ]}
                value={mode}
                onValueChange={setMode}
                name="snowluma-mode"
            />
            {mode === 'hot_start' && (
                <p className="rounded-sm bg-inset px-3 py-2 text-2xs leading-relaxed text-text-tertiary">
                    启动时会扫描当前所有 QQ.exe 进程，找到登录账号 ==
                    {' '}
                    <span className="font-mono">{data.QQID || '未填写'}</span>
                    {' '}
                    的那个进程并注入。请在 Bot 启动前先在 QQ 客户端登录此账号；找不到时启动会失败并给出提示。
                </p>
            )}
        </>
    );
}
