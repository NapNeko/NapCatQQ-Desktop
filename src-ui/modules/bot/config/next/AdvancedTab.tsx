// 高级 Tab：按底座显隐；全局 WebUI 仅 SnowLuma。

import {
    Switch,
    Select,
    TextField,
    NumberField,
    Checkbox,
    FormSection,
} from '../../../../shared/ui';
import type { AdvancedConfig } from '../../../../core/ipc/generated/domain/AdvancedConfig';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
import type { LogLevel } from '../../../../core/ipc/generated/domain/LogLevel';
import type { BypassConfig } from '../../../../core/ipc/generated/domain/BypassConfig';
import type { StatusCommandConfig } from '../../../../core/ipc/generated/domain/StatusCommandConfig';
import type { SnowLumaAppConfig } from '../../../../core/ipc/generated/domain/SnowLumaAppConfig';
import { SnowLumaGlobalWebuiSection } from './SnowLumaGlobalWebuiSection';

interface AdvancedTabProps {
    data: AdvancedConfig;
    onChange: (patch: Partial<AdvancedConfig>) => void;
    backendType: BackendType;
    statusCommand: StatusCommandConfig | null;
    onStatusCommandChange: (patch: Partial<StatusCommandConfig>) => void;
    snowlumaAppConfig: SnowLumaAppConfig;
    onSnowlumaAppConfigChange: (next: SnowLumaAppConfig) => void;
    webuiPasswordTakeover: boolean;
    onWebuiPasswordTakeoverChange: (v: boolean) => void;
    snowlumaAppLoadError?: string | null;
    snowlumaAppLoading?: boolean;
    remoteTarget?: boolean;
    localNative?: boolean;
}

const LOG_LEVEL_ITEMS = [
    { value: 'trace' as LogLevel, label: 'trace（最详尽）' },
    { value: 'debug' as LogLevel, label: 'debug（排查问题用）' },
    { value: 'info' as LogLevel, label: 'info（默认）' },
    { value: 'warn' as LogLevel, label: 'warn' },
    { value: 'error' as LogLevel, label: 'error（仅错误）' },
];

const PACKET_BACKEND_ITEMS = [
    { value: 'auto', label: 'auto' },
    { value: 'disable', label: 'disable' },
];

interface BypassFieldMeta {
    key: keyof BypassConfig;
    label: string;
}

const BYPASS_FIELDS: ReadonlyArray<BypassFieldMeta> = [
    { key: 'hook', label: 'Hook' },
    { key: 'window', label: 'Window' },
    { key: 'module', label: 'Module' },
    { key: 'process', label: 'Process' },
    { key: 'container', label: 'Container' },
    { key: 'js', label: 'JS' },
];

const DEFAULT_STATUS_COMMAND: StatusCommandConfig = {
    enabled: true,
    swallow: false,
    cooldownSeconds: 5,
};

export function AdvancedTab({
    data,
    onChange,
    backendType,
    statusCommand,
    onStatusCommandChange,
    snowlumaAppConfig,
    onSnowlumaAppConfigChange,
    snowlumaAppLoadError,
    snowlumaAppLoading,
    webuiPasswordTakeover,
    onWebuiPasswordTakeoverChange,
    remoteTarget = false,
    localNative = false,
}: AdvancedTabProps) {
    const isSnowLuma = backendType === 'snowluma';
    const sc = statusCommand ?? DEFAULT_STATUS_COMMAND;

    const handleBypass = (key: keyof BypassConfig, value: boolean) => {
        onChange({ bypass: { ...data.bypass, [key]: value } });
    };

    return (
        <div className="flex w-full flex-col gap-6">
            <FormSection title="桌面端集成">
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                        <div className="w-full">
                            <Switch
                                label="随桌面端自动启动"
                                hint="打开软件时自动拉起此 Bot"
                                checked={data.autoStart}
                                onCheckedChange={(v) => onChange({ autoStart: v })}
                            />
                        </div>
                    </div>
                    <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                        <div className="w-full">
                            <Switch
                                label="掉线时发送通知"
                                hint={
                                    isSnowLuma
                                        ? '需开启设置里的推送通道；即时生效'
                                        : '需开启设置里的推送通道；重启后生效'
                                }
                                checked={data.offlineNotice}
                                onCheckedChange={(v) => onChange({ offlineNotice: v })}
                            />
                        </div>
                    </div>
                </div>
            </FormSection>

            {isSnowLuma ? (
                <>
                    <SnowLumaGlobalWebuiSection
                        value={snowlumaAppConfig}
                        onChange={onSnowlumaAppConfigChange}
                        loadError={snowlumaAppLoadError}
                        loading={snowlumaAppLoading}
                    />

                    <FormSection title="扫码与鉴权">
                        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                            {localNative && (
                                <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                                    <div className="w-full">
                                        <Switch
                                            label="Windows 简化扫码（全局）"
                                            hint="下次冷启动生效；需保持桌面会话可用"
                                            checked={snowlumaAppConfig.snowlumaLocalQrLoginEnabled}
                                            disabled={snowlumaAppLoading || !!snowlumaAppLoadError}
                                            onCheckedChange={(enabled) =>
                                                onSnowlumaAppConfigChange({
                                                    ...snowlumaAppConfig,
                                                    snowlumaLocalQrLoginEnabled: enabled,
                                                })
                                            }
                                        />
                                    </div>
                                </div>
                            )}

                            {remoteTarget && (
                                <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                                    <div className="w-full">
                                        <Switch
                                            label="简化扫码登录（全局）"
                                            hint="下次冷启动 QQ 生效；登录后自动关闭通道"
                                            checked={snowlumaAppConfig.snowlumaRemoteQrLoginEnabled}
                                            disabled={snowlumaAppLoading || !!snowlumaAppLoadError}
                                            onCheckedChange={(enabled) =>
                                                onSnowlumaAppConfigChange({
                                                    ...snowlumaAppConfig,
                                                    snowlumaRemoteQrLoginEnabled: enabled,
                                                })
                                            }
                                        />
                                    </div>
                                </div>
                            )}

                            <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                                <div className="w-full">
                                    <Switch
                                        label="启动时覆盖远端 WebUI 密码"
                                        hint="仅远端直接运行生效；关闭则保留远端原密码"
                                        checked={webuiPasswordTakeover}
                                        onCheckedChange={onWebuiPasswordTakeoverChange}
                                    />
                                </div>
                            </div>
                        </div>
                    </FormSection>

                    <FormSection title="SnowLuma 协议与内置命令">
                        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                            <div className="flex flex-col justify-between gap-3 rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-3">
                                <Switch
                                    label="启用 #sl 状态命令"
                                    hint="响应群聊或私聊中的 #sl 探测指令"
                                    checked={sc.enabled}
                                    onCheckedChange={(v) => onStatusCommandChange({ enabled: v })}
                                />
                                <div className="border-t border-border-subtle/70 pt-2.5">
                                    <Switch
                                        label="命中后不转发给下游（swallow）"
                                        hint="仅本地回复，不再投递给下游应用"
                                        checked={sc.swallow}
                                        onCheckedChange={(v) =>
                                            onStatusCommandChange({ swallow: v })
                                        }
                                        disabled={!sc.enabled}
                                    />
                                </div>
                            </div>
                            <div className="flex flex-col justify-between gap-2 rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-3">
                                <div className="space-y-0.5">
                                    <div className="text-sm font-medium text-text">
                                        回复冷却（秒）
                                    </div>
                                    <div className="text-2xs text-text-tertiary">
                                        限制同一会话触发 #sl 的最小间隔，0 表示不限制
                                    </div>
                                </div>
                                <NumberField
                                    aria-label="回复冷却（秒）"
                                    value={sc.cooldownSeconds}
                                    onValueChange={(v) =>
                                        onStatusCommandChange({
                                            cooldownSeconds: v ?? 0,
                                        })
                                    }
                                    min={0}
                                    disabled={!sc.enabled}
                                />
                            </div>
                        </div>
                    </FormSection>
                </>
            ) : (
                <>
                    <FormSection title="OneBot 行为">
                        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                            <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                                <div className="w-full">
                                    <Switch
                                        label="启用本地文件到 URL"
                                        hint="上报时把本地文件路径转成 URL"
                                        checked={data.enableLocalFile2Url}
                                        onCheckedChange={(v) =>
                                            onChange({ enableLocalFile2Url: v })
                                        }
                                    />
                                </div>
                            </div>
                            <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                                <div className="w-full">
                                    <Switch
                                        label="启用合并消息上报解析"
                                        hint="合并转发展开成普通消息列表上报"
                                        checked={data.parseMultMsg}
                                        onCheckedChange={(v) => onChange({ parseMultMsg: v })}
                                    />
                                </div>
                            </div>
                        </div>
                    </FormSection>

                    <FormSection title="核心日志配置" description="修改后需重启 Bot">
                        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                            <div className="flex flex-col gap-3 rounded-md border border-border-subtle/80 bg-field/35 p-3.5">
                                <Switch
                                    label="文件日志"
                                    checked={data.fileLog}
                                    onCheckedChange={(v) => onChange({ fileLog: v })}
                                />
                                <Select
                                    label="文件日志等级"
                                    items={LOG_LEVEL_ITEMS}
                                    value={data.fileLogLevel}
                                    disabled={!data.fileLog}
                                    onValueChange={(v) => onChange({ fileLogLevel: v })}
                                />
                            </div>
                            <div className="flex flex-col gap-3 rounded-md border border-border-subtle/80 bg-field/35 p-3.5">
                                <Switch
                                    label="控制台日志"
                                    checked={data.consoleLog}
                                    onCheckedChange={(v) => onChange({ consoleLog: v })}
                                />
                                <Select
                                    label="控制台日志等级"
                                    items={LOG_LEVEL_ITEMS}
                                    value={data.consoleLogLevel}
                                    disabled={!data.consoleLog}
                                    onValueChange={(v) => onChange({ consoleLogLevel: v })}
                                />
                            </div>
                        </div>
                    </FormSection>

                    <FormSection
                        title="封包后端与反检测"
                        description="修改后需重启 Bot；封包后端一般保持 auto"
                    >
                        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                            <div className="flex flex-col justify-between gap-3 rounded-md border border-border-subtle/80 bg-field/35 p-3.5">
                                <Select
                                    label="封包后端 (PacketBackend)"
                                    items={PACKET_BACKEND_ITEMS}
                                    value={data.packetBackend || 'auto'}
                                    onValueChange={(v) => onChange({ packetBackend: v })}
                                />
                                <TextField
                                    label="封包服务地址 (可选)"
                                    value={data.packetServer}
                                    onValueChange={(v) => onChange({ packetServer: v })}
                                    placeholder="留空则使用进程内置后端"
                                    className="font-mono"
                                />
                            </div>
                            <div className="flex flex-col justify-between gap-3 rounded-md border border-border-subtle/80 bg-field/35 p-3.5">
                                <div className="space-y-2">
                                    <span className="block text-xs font-medium text-text-secondary">
                                        反检测绕过 (Bypass)
                                    </span>
                                    <div className="grid grid-cols-3 gap-2.5">
                                        {BYPASS_FIELDS.map(({ key, label }) => (
                                            <Checkbox
                                                key={key}
                                                label={label}
                                                checked={data.bypass[key]}
                                                onCheckedChange={(v) => handleBypass(key, v)}
                                            />
                                        ))}
                                    </div>
                                </div>
                                <div className="border-t border-border-subtle/70 pt-2.5">
                                    <Switch
                                        label="o3HookMode"
                                        checked={data.o3HookMode === 1}
                                        onCheckedChange={(v) => onChange({ o3HookMode: v ? 1 : 0 })}
                                    />
                                </div>
                            </div>
                        </div>
                    </FormSection>
                </>
            )}
        </div>
    );
}
