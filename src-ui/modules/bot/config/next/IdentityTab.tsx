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
import type { BotBasicConfig } from '../../../../core/ipc/generated/domain/BotBasicConfig';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
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

const TIME_UNIT_ITEMS = [
    { value: 'm' as TimeUnit, label: '分钟' },
    { value: 'h' as TimeUnit, label: '小时' },
    { value: 'd' as TimeUnit, label: '天' },
    { value: 'mon' as TimeUnit, label: '月' },
    { value: 'year' as TimeUnit, label: '年' },
];

export function IdentityTab({ data, onChange, isEditMode, isRunning }: IdentityTabProps) {
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
                description="Bot 引擎实际跑在哪台机器上"
            >
                <RadioGroup
                    items={RUNTIME_ITEMS}
                    value={data.runtime_target}
                    onValueChange={(v) => onChange({ runtime_target: v })}
                    orientation="horizontal"
                    name="runtime-target"
                />
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
