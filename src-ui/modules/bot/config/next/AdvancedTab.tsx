// 高级 Tab：按底座显隐；全局 WebUI 仅 SnowLuma。

import { Switch, Select, TextField, NumberField, Checkbox, FormSection } from '../../../../shared/ui';
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
}: AdvancedTabProps) {
    const isSnowLuma = backendType === 'snowluma';
    const sc = statusCommand ?? DEFAULT_STATUS_COMMAND;

    const handleBypass = (key: keyof BypassConfig, value: boolean) => {
        onChange({ bypass: { ...data.bypass, [key]: value } });
    };

    return (
        <div className="flex flex-col gap-8">
            {isSnowLuma && (
                <SnowLumaGlobalWebuiSection
                    value={snowlumaAppConfig}
                    onChange={onSnowlumaAppConfigChange}
                    loadError={snowlumaAppLoadError}
                    loading={snowlumaAppLoading}
                />
            )}

            {isSnowLuma && (
                <FormSection
                    title="WebUI 密码接管"
                    description="仅远端 Native"
                >
                    <Switch
                        label="启动时覆盖 WebUI 密码"
                        hint="填了固定密码就用固定的，否则生成新的；原密码立即失效。关闭则不改远端配置。"
                        checked={webuiPasswordTakeover}
                        onCheckedChange={onWebuiPasswordTakeoverChange}
                    />
                </FormSection>
            )}

            <FormSection title="桌面端集成">
                <Switch
                    label="桌面端启动时自动拉起此 Bot"
                    checked={data.autoStart}
                    onCheckedChange={(v) => onChange({ autoStart: v })}
                />
                <Switch
                    label="掉线时发送通知"
                    hint={
                        isSnowLuma
                            ? '需同时打开设置里的推送通道；改完后无需重启。'
                            : '需同时打开设置里的推送通道；改完后重启该 Bot。'
                    }
                    checked={data.offlineNotice}
                    onCheckedChange={(v) => onChange({ offlineNotice: v })}
                />
            </FormSection>

            {isSnowLuma && (
                <FormSection title="SnowLuma 协议与内置命令">
                    <Switch
                        label="启用 #sl 状态命令"
                        checked={sc.enabled}
                        onCheckedChange={(v) => onStatusCommandChange({ enabled: v })}
                    />
                    <Switch
                        label="命中后不转发给下游（swallow）"
                        hint="开启后 #sl 不再投递给已连接的 Bot，仍会本地回复"
                        checked={sc.swallow}
                        onCheckedChange={(v) => onStatusCommandChange({ swallow: v })}
                        disabled={!sc.enabled}
                    />
                    <NumberField
                        label="回复冷却（秒）"
                        value={sc.cooldownSeconds}
                        onValueChange={(v) =>
                            onStatusCommandChange({
                                cooldownSeconds: v ?? 0,
                            })
                        }
                        min={0}
                        disabled={!sc.enabled}
                        hint="同一会话在该秒数内重复 #sl 不再回复；0 表示不限制"
                    />
                </FormSection>
            )}

            {!isSnowLuma && (
                <>
                    <FormSection title="OneBot 行为">
                        <Switch
                            label="启用本地文件到 URL"
                            hint="上报时把本地文件路径转成 URL"
                            checked={data.enableLocalFile2Url}
                            onCheckedChange={(v) => onChange({ enableLocalFile2Url: v })}
                        />
                        <Switch
                            label="启用合并消息上报解析"
                            hint="合并转发展开成普通消息列表上报"
                            checked={data.parseMultMsg}
                            onCheckedChange={(v) => onChange({ parseMultMsg: v })}
                        />
                    </FormSection>

                    <FormSection title="核心配置" description="修改后需重启 Bot">
                        <Switch
                            label="文件日志"
                            checked={data.fileLog}
                            onCheckedChange={(v) => onChange({ fileLog: v })}
                        />
                        <Switch
                            label="控制台日志"
                            checked={data.consoleLog}
                            onCheckedChange={(v) => onChange({ consoleLog: v })}
                        />
                        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                            <Select
                                label="文件日志等级"
                                items={LOG_LEVEL_ITEMS}
                                value={data.fileLogLevel}
                                onValueChange={(v) => onChange({ fileLogLevel: v })}
                            />
                            <Select
                                label="控制台日志等级"
                                items={LOG_LEVEL_ITEMS}
                                value={data.consoleLogLevel}
                                onValueChange={(v) => onChange({ consoleLogLevel: v })}
                            />
                        </div>
                    </FormSection>

                    <FormSection title="反检测开关" description="修改后需重启 Bot">
                        <div className="grid grid-cols-1 gap-x-4 gap-y-2 sm:grid-cols-2 lg:grid-cols-3">
                            {BYPASS_FIELDS.map(({ key, label }) => (
                                <Checkbox
                                    key={key}
                                    label={label}
                                    checked={data.bypass[key]}
                                    onCheckedChange={(v) => handleBypass(key, v)}
                                />
                            ))}
                        </div>
                        <div className="mt-1 border-t border-border-subtle pt-3">
                            <Switch
                                label="o3HookMode"
                                checked={data.o3HookMode === 1}
                                onCheckedChange={(v) => onChange({ o3HookMode: v ? 1 : 0 })}
                            />
                        </div>
                    </FormSection>

                    <FormSection title="封包后端 (PacketBackend)" description="除非接入独立封包服务，一般保持 auto">
                        <Select
                            label="后端模式"
                            items={PACKET_BACKEND_ITEMS}
                            value={data.packetBackend || 'auto'}
                            onValueChange={(v) => onChange({ packetBackend: v })}
                        />
                        <TextField
                            label="封包服务地址 (可选)"
                            value={data.packetServer}
                            onValueChange={(v) => onChange({ packetServer: v })}
                            placeholder="留空则使用进程内置后端"
                        />
                    </FormSection>
                </>
            )}
        </div>
    );
}