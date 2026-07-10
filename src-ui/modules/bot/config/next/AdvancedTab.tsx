// Bot 配置页 — 高级 Tab：按底座动态显隐；全局 WebUI 仅 SnowLuma 底座显示。

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
    { value: 'auto', label: 'auto（自动选择）' },
    { value: 'disable', label: 'disable（关闭 PacketBackend）' },
];

interface BypassFieldMeta {
    key: keyof BypassConfig;
    label: string;
    description: string;
}

const BYPASS_FIELDS: ReadonlyArray<BypassFieldMeta> = [
    { key: 'hook', label: 'Hook', description: 'hook 特征隐藏' },
    { key: 'window', label: 'Window', description: '窗口伪造' },
    { key: 'module', label: 'Module', description: '加载模块隐藏' },
    { key: 'process', label: 'Process', description: '进程反检测' },
    { key: 'container', label: 'Container', description: '容器反检测' },
    { key: 'js', label: 'JS', description: 'JS 反检测' },
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

            <FormSection
                title="桌面端集成"
                description="NapCatQQ Desktop 对此实例的行为；协议端是否消费因底座而异"
            >
                <Switch
                    label="桌面端启动时自动拉起此 Bot"
                    hint="勾上后软件每次开机就尝试启动这个实例"
                    checked={data.autoStart}
                    onCheckedChange={(v) => onChange({ autoStart: v })}
                />
                <Switch
                    label="掉线时发送通知"
                    hint={
                        isSnowLuma
                            ? 'SnowLuma 登录态从已登录变为断开时触发；需同时打开设置里对应通道（桌面 Toast / Webhook / 邮件 / OneBot）。改完后无需重启 daemon，下一轮状态变化即生效。'
                            : 'NapCat 登录轮询检测到在线→离线边沿时触发；需同时打开设置里对应通道。改完后请重启该 Bot，让 LoginPoller 重新加载开关。'
                    }
                    checked={data.offlineNotice}
                    onCheckedChange={(v) => onChange({ offlineNotice: v })}
                />
            </FormSection>

            {isSnowLuma && (
                <FormSection
                    title="SnowLuma 协议与内置命令"
                    description="写入此 Bot 的 onebot_<QQ>.json；音乐签名在「身份 → 附加服务」"
                >
                    <Switch
                        label="启用 #sl 状态命令"
                        hint="收到纯文本 #sl 时回复 SnowLuma 版本与运行信息"
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
                    <FormSection title="OneBot 行为" description="协议层上报与文件转换开关">
                        <Switch
                            label="启用本地文件到 URL"
                            hint="OneBot 上报时把本地文件路径转成可访问的 URL"
                            checked={data.enableLocalFile2Url}
                            onCheckedChange={(v) => onChange({ enableLocalFile2Url: v })}
                        />
                        <Switch
                            label="启用合并消息上报解析"
                            hint="把合并转发消息展开成普通消息列表上报给客户端"
                            checked={data.parseMultMsg}
                            onCheckedChange={(v) => onChange({ parseMultMsg: v })}
                        />
                    </FormSection>

                    <FormSection
                        title="核心配置"
                        description="控制 NapCat 框架底层的核心行为；修改后需重启 Bot 生效"
                    >
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

                    <FormSection title="反检测开关" description="Napi2Native 反检测；修改后需重启 Bot 生效">
                        <div className="grid grid-cols-1 gap-x-4 gap-y-2 sm:grid-cols-2 lg:grid-cols-3">
                            {BYPASS_FIELDS.map(({ key, label, description }) => (
                                <Checkbox
                                    key={key}
                                    label={label}
                                    hint={description}
                                    checked={data.bypass[key]}
                                    onCheckedChange={(v) => handleBypass(key, v)}
                                />
                            ))}
                        </div>
                        <div className="mt-1 border-t border-border-subtle pt-3">
                            <Switch
                                label="o3HookMode"
                                hint="O3 Hook 模式"
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