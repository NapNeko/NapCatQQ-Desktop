// 高级 Tab：日志输出 + 反检测 + 封包后端 + 桌面端附加项。
//
// 字段对齐口径，以 NapCat 官方源为准（packages/napcat-core/helper/config.ts +
// packages/napcat-webui-frontend/src/pages/dashboard/config/{core,bypass,onebot}.tsx）。
// 分组直接照搬 NapCat WebUI 的归属，没有的字段不再瞎编：
//
//   桌面端集成      autoStart / offlineNotice            (NCD-Desktop 自家字段)
//   OneBot 行为     enableLocalFile2Url / parseMultMsg   (NapCat onebot 配置)
//   核心配置        fileLog / consoleLog + 各自日志等级  (NapCat napcat.json)
//   反检测开关      bypass.{...} + o3HookMode            (NapCat napcat.json)
//   封包后端        packetBackend (auto/disable) + packetServer
//
// SnowLuma 底座不消费 NapCat 三段，只保留桌面端集成。

import { Switch, Select, TextField, Checkbox, FormSection } from '../../../../shared/ui';
import type { AdvancedConfig } from '../../../../core/ipc/generated/domain/AdvancedConfig';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
import type { LogLevel } from '../../../../core/ipc/generated/domain/LogLevel';
import type { BypassConfig } from '../../../../core/ipc/generated/domain/BypassConfig';

interface AdvancedTabProps {
    data: AdvancedConfig;
    onChange: (patch: Partial<AdvancedConfig>) => void;
    backendType: BackendType;
}

const LOG_LEVEL_ITEMS = [
    { value: 'trace' as LogLevel, label: 'trace（最详尽）' },
    { value: 'debug' as LogLevel, label: 'debug（排查问题用）' },
    { value: 'info' as LogLevel, label: 'info（默认）' },
    { value: 'warn' as LogLevel, label: 'warn' },
    { value: 'error' as LogLevel, label: 'error（仅错误）' },
];

// packetBackend 合法值取自 napcat-core/apis/packet.ts:62（disable 走分支），
// auto 是 schema 默认值。其它字符串 NapCat 内部当 fallback。
const PACKET_BACKEND_ITEMS = [
    { value: 'auto', label: 'auto（自动选择）' },
    { value: 'disable', label: 'disable（关闭 PacketBackend）' },
];

// bypass 文案 1:1 对齐 napcat-webui-frontend bypass.tsx 的 SwitchCard label /
// description，避免我们这边再造一份"听上去像但跟官方不同"的中文翻译。
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

export function AdvancedTab({ data, onChange, backendType }: AdvancedTabProps) {
    const isSnowLuma = backendType === 'snowluma';

    const handleBypass = (key: keyof BypassConfig, value: boolean) => {
        onChange({ bypass: { ...data.bypass, [key]: value } });
    };

    return (
        <div className="flex flex-col gap-8">
            <FormSection
                title="桌面端集成"
                description="NapCatQQ Desktop 自身对此实例的行为；NapCat / SnowLuma 协议端不消费这两项"
            >
                <Switch
                    label="桌面端启动时自动拉起此 Bot"
                    hint="勾上后软件每次开机就尝试启动这个实例"
                    checked={data.autoStart}
                    onCheckedChange={(v) => onChange({ autoStart: v })}
                />
                <Switch
                    label="掉线时下发桌面通知"
                    hint="检测到 Bot 离线时弹一条系统通知，方便长时间无人值守"
                    checked={data.offlineNotice}
                    onCheckedChange={(v) => onChange({ offlineNotice: v })}
                />
            </FormSection>

            {!isSnowLuma && (
                <>
                    <FormSection
                        title="OneBot 行为"
                        description="协议层上报与文件转换开关"
                    >
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
                            hint="登录后日志写入本地文件"
                            checked={data.fileLog}
                            onCheckedChange={(v) => onChange({ fileLog: v })}
                        />
                        <Switch
                            label="控制台日志"
                            hint="终端标准输出实时打印日志"
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

                    <FormSection
                        title="反检测开关"
                        description="控制 Napi2Native 模块的反检测功能；修改后需重启 Bot 生效"
                    >
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
                                hint="O3 Hook 模式：开启后启用更深一层的 native hook"
                                checked={data.o3HookMode === 1}
                                onCheckedChange={(v) => onChange({ o3HookMode: v ? 1 : 0 })}
                            />
                        </div>
                    </FormSection>

                    <FormSection
                        title="封包后端 (PacketBackend)"
                        description="协议封包处理后端；除非接入 NapCat.Packet 独立服务，一般保持 auto"
                    >
                        <Select
                            label="后端模式"
                            items={PACKET_BACKEND_ITEMS}
                            value={data.packetBackend || 'auto'}
                            onValueChange={(v) => onChange({ packetBackend: v })}
                            hint="disable 表示完全关闭 PacketBackend，部分高级 API 将不可用"
                        />
                        <TextField
                            label="封包服务地址 (可选)"
                            value={data.packetServer}
                            onValueChange={(v) => onChange({ packetServer: v })}
                            placeholder="留空则使用进程内置后端"
                            hint="形如 http://127.0.0.1:8086，仅当部署了独立的 NapCat.Packet 服务时填写"
                        />
                    </FormSection>
                </>
            )}

            {isSnowLuma && (
                <FormSection>
                    <p className="rounded-sm border border-dashed border-border-subtle bg-canvas/60 px-4 py-3 text-2xs leading-relaxed text-text-tertiary">
                        SnowLuma 底座不消费 NapCat 的 OneBot 行为 / 核心配置 / 反检测 / 封包后端等字段，
                        因此这些分组已隐藏。如需调整 SnowLuma 自身的高级选项，
                        请直接编辑 SnowLuma runtime 目录下的 config 文件。
                    </p>
                </FormSection>
            )}
        </div>
    );
}
