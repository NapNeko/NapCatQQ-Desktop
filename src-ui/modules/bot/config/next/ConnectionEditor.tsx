// 单个连接的就地编辑器。inline 展开（不弹 Dialog）—— 用户可以在主页面同时
// 看到列表里其他连接，避免 Dialog 模态框阻断浏览的反直觉问题。
//
// 设计要点：
//   - 公共字段（名称 / token / 消息格式 / debug）在前，差异字段在中段，
//     "启用此连接" 开关放在 Header 右侧（与"取消/保存"分离，不混入提交流）
//   - 校验全走 core/domain/bot/connections.ts，UI 只负责调度
//   - 校验失败时把 reason 留在编辑器内（小红条）+ 同时 push 全局 InfoBar
//     就近提示更直接，全局 InfoBar 仅作 fallback（用户切走时还能看到）

import { useEffect, useState } from 'react';
import {
    Button,
    TextField,
    NumberField,
    Switch,
    Select,
    Checkbox,
} from '../../../../shared/ui';
import {
    type ConnectionKind,
    type ConnectionConfig,
    validateConnection,
} from '../../../../core/domain/bot/connections';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
import type { HttpServerConfig } from '../../../../core/ipc/generated/domain/HttpServerConfig';
import type { HttpSseServerConfig } from '../../../../core/ipc/generated/domain/HttpSseServerConfig';
import type { HttpClientConfig } from '../../../../core/ipc/generated/domain/HttpClientConfig';
import type { WebsocketServerConfig } from '../../../../core/ipc/generated/domain/WebsocketServerConfig';
import type { WebsocketClientConfig } from '../../../../core/ipc/generated/domain/WebsocketClientConfig';
import type { MessagePostFormat } from '../../../../core/ipc/generated/domain/MessagePostFormat';
import type { WsRole } from '../../../../core/ipc/generated/domain/WsRole';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';

const MSG_FORMAT_ITEMS = [
    { value: 'array' as MessagePostFormat, label: 'Array（结构化数组，推荐）' },
    { value: 'string' as MessagePostFormat, label: 'String（纯文本）' },
];

const WS_ROLE_ITEMS = [
    { value: 'Universal' as WsRole, label: 'Universal（全双工，API + 事件）' },
    { value: 'Api' as WsRole, label: 'API（仅 API 调用）' },
    { value: 'Event' as WsRole, label: 'Event（仅事件投递）' },
];

interface ConnectionEditorProps {
    kind: ConnectionKind;
    initialData: ConnectionConfig;
    /** 不含自己。重名校验用。 */
    existingNames: string[];
    backendType: BackendType;
    onSave: (data: ConnectionConfig) => void;
    onCancel: () => void;
}

export function ConnectionEditor({
    kind,
    initialData,
    existingNames,
    backendType,
    onSave,
    onCancel,
}: ConnectionEditorProps) {
    const [data, setData] = useState<ConnectionConfig>(initialData);
    const [error, setError] = useState<string | null>(null);

    // 切换不同行编辑时同步 reset。
    useEffect(() => {
        setData(initialData);
        setError(null);
    }, [initialData]);

    // 类型安全的局部 patch：保持 data 的具体类型不被 widen。
    const patch = <K extends keyof ConnectionConfig>(field: K, value: ConnectionConfig[K]) => {
        setData((prev) => ({ ...prev, [field]: value } as ConnectionConfig));
    };

    const handleSave = () => {
        // 名称 trim 后再校验：避免用户输入末尾空格导致重名误判。
        const trimmedName = data.name.trim();
        const finalData = { ...data, name: trimmedName } as ConnectionConfig;

        // url 字段（HttpClient / WsClient）也要 trim
        if ('url' in finalData) {
            (finalData as HttpClientConfig | WebsocketClientConfig).url = (
                finalData as HttpClientConfig | WebsocketClientConfig
            ).url.trim();
        }

        const result = validateConnection(kind, finalData, existingNames);
        if (!result.ok) {
            setError(result.reason);
            // 推送到全局 InfoBar
            pushInfoBar({
                tone: 'danger',
                title: '连接配置有误',
                content: result.reason,
                autoDismissMs: 4000,
            });
            return;
        }
        onSave(finalData);
    };

    const isSnowLuma = backendType === 'snowluma';

    return (
        <div className="flex flex-col gap-3">
            {/* 公共字段 */}
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <TextField
                    label="连接名称"
                    required
                    value={data.name}
                    onValueChange={(v) => patch('name', v)}
                    placeholder="例如：koishi-http"
                    error={error?.includes('名称') ? error : undefined}
                />
                <Select
                    label="消息上报格式"
                    items={MSG_FORMAT_ITEMS}
                    value={data.messagePostFormat}
                    onValueChange={(v) => patch('messagePostFormat', v)}
                />
            </div>

            <KindSpecificFields kind={kind} data={data} patch={patch} isSnowLuma={isSnowLuma} error={error} />

            {/* token + debug 放尾部，频次低 */}
            <TextField
                label="鉴权 Token"
                value={data.token}
                onValueChange={(v) => patch('token', v)}
                placeholder="留空则不需鉴权"
                hint="外部应用请求时携带在 Authorization Header / URL Query"
            />
            <Checkbox
                label="开启调试输出"
                checked={data.debug}
                onCheckedChange={(v) => patch('debug', v)}
            />

            {/* Footer：启用开关靠左，操作按钮靠右 */}
            <div className="mt-3 flex items-center justify-between gap-3 border-t border-border-subtle pt-4">
                <Switch
                    checked={data.enable}
                    onCheckedChange={(v) => patch('enable', v)}
                    label="启用此连接"
                />
                <div className="flex items-center gap-2">
                    <Button variant="ghost" size="sm" onClick={onCancel}>
                        取消
                    </Button>
                    <Button variant="primary" size="sm" onClick={handleSave}>
                        保存连接
                    </Button>
                </div>
            </div>
        </div>
    );
}

interface KindSpecificFieldsProps {
    kind: ConnectionKind;
    data: ConnectionConfig;
    patch: <K extends keyof ConnectionConfig>(field: K, value: ConnectionConfig[K]) => void;
    isSnowLuma: boolean;
    error: string | null;
}

function KindSpecificFields({ kind, data, patch, isSnowLuma, error }: KindSpecificFieldsProps) {
    switch (kind) {
        case 'httpServer':
            return <HttpServerFields data={data as HttpServerConfig} patch={patch} isSnowLuma={isSnowLuma} error={error} />;
        case 'httpSseServer':
            return <HttpSseServerFields data={data as HttpSseServerConfig} patch={patch} error={error} />;
        case 'httpClient':
            return <HttpClientFields data={data as HttpClientConfig} patch={patch} isSnowLuma={isSnowLuma} error={error} />;
        case 'websocketServer':
            return <WebsocketServerFields data={data as WebsocketServerConfig} patch={patch} error={error} />;
        case 'websocketClient':
            return <WebsocketClientFields data={data as WebsocketClientConfig} patch={patch} error={error} />;
    }
}

// ================== 5 类差异字段子组件 ==================
// 单独抽出来：1) Editor 主体短；2) 保留各 kind 的字段顺序差异（不强求对称）。

type Patch = <K extends keyof ConnectionConfig>(field: K, value: ConnectionConfig[K]) => void;

function HttpServerFields({
    data,
    patch,
    isSnowLuma,
    error,
}: {
    data: HttpServerConfig;
    patch: Patch;
    isSnowLuma: boolean;
    error: string | null;
}) {
    return (
        <>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                <TextField
                    label="监听 IP (Host)"
                    required
                    value={data.host}
                    onValueChange={(v) => patch('host' as keyof ConnectionConfig, v as never)}
                    placeholder="0.0.0.0"
                    className="sm:col-span-2"
                />
                <NumberField
                    label="端口"
                    required
                    value={data.port}
                    onValueChange={(v) => patch('port' as keyof ConnectionConfig, (v ?? 0) as never)}
                    placeholder="3000"
                    min={1}
                    max={65535}
                    error={error?.includes('端口') ? error : undefined}
                />
            </div>
            <TextField
                label="监听路径"
                value={data.path}
                onValueChange={(v) => patch('path' as keyof ConnectionConfig, v as never)}
                placeholder="/"
                hint={isSnowLuma ? 'SnowLuma 服务端监听的具体 HTTP 路由' : undefined}
            />
            {!isSnowLuma && (
                <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
                    <Checkbox
                        label="允许跨域请求 (CORS)"
                        checked={data.enableCors}
                        onCheckedChange={(v) => patch('enableCors' as keyof ConnectionConfig, v as never)}
                    />
                    <Checkbox
                        label="兼任 WebSocket 握手"
                        checked={data.enableWebsocket}
                        onCheckedChange={(v) => patch('enableWebsocket' as keyof ConnectionConfig, v as never)}
                    />
                </div>
            )}
        </>
    );
}

function HttpSseServerFields({ data, patch, error }: { data: HttpSseServerConfig; patch: Patch; error: string | null }) {
    return (
        <>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                <TextField
                    label="监听 IP (Host)"
                    required
                    value={data.host}
                    onValueChange={(v) => patch('host' as keyof ConnectionConfig, v as never)}
                    placeholder="0.0.0.0"
                    className="sm:col-span-2"
                />
                <NumberField
                    label="端口"
                    required
                    value={data.port}
                    onValueChange={(v) => patch('port' as keyof ConnectionConfig, (v ?? 0) as never)}
                    placeholder="3001"
                    min={1}
                    max={65535}
                    error={error?.includes('端口') ? error : undefined}
                />
            </div>
            <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
                <Checkbox
                    label="允许跨域请求"
                    checked={data.enableCors}
                    onCheckedChange={(v) => patch('enableCors' as keyof ConnectionConfig, v as never)}
                />
                <Checkbox
                    label="兼任 WebSocket 握手"
                    checked={data.enableWebsocket}
                    onCheckedChange={(v) => patch('enableWebsocket' as keyof ConnectionConfig, v as never)}
                />
                <Checkbox
                    label="上报 Bot 自身消息"
                    checked={data.reportSelfMessage}
                    onCheckedChange={(v) => patch('reportSelfMessage' as keyof ConnectionConfig, v as never)}
                />
            </div>
        </>
    );
}

function HttpClientFields({
    data,
    patch,
    isSnowLuma,
    error,
}: {
    data: HttpClientConfig;
    patch: Patch;
    isSnowLuma: boolean;
    error: string | null;
}) {
    return (
        <>
            <TextField
                label="上报 Webhook URL"
                required
                value={data.url}
                onValueChange={(v) => patch('url' as keyof ConnectionConfig, v as never)}
                placeholder="http://127.0.0.1:8080/webhook"
                hint="必须以 http:// 或 https:// 开头"
                error={error?.includes('Webhook URL') || error?.includes('URL') ? error : undefined}
            />
            <Checkbox
                label="上报 Bot 自身发出的消息"
                checked={data.reportSelfMessage}
                onCheckedChange={(v) => patch('reportSelfMessage' as keyof ConnectionConfig, v as never)}
            />
            {isSnowLuma && (
                <NumberField
                    label="请求超时 (ms)"
                    value={data.timeoutMs ?? null}
                    onValueChange={(v) =>
                        patch('timeoutMs' as keyof ConnectionConfig, (v ?? undefined) as never)
                    }
                    placeholder="留空使用引擎默认值"
                    hint="SnowLuma 独有可选字段"
                />
            )}
        </>
    );
}

function WebsocketServerFields({
    data,
    patch,
    error,
}: {
    data: WebsocketServerConfig;
    patch: Patch;
    error: string | null;
}) {
    return (
        <>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                <TextField
                    label="监听 IP (Host)"
                    required
                    value={data.host}
                    onValueChange={(v) => patch('host' as keyof ConnectionConfig, v as never)}
                    placeholder="0.0.0.0"
                    className="sm:col-span-2"
                />
                <NumberField
                    label="端口"
                    required
                    value={data.port}
                    onValueChange={(v) => patch('port' as keyof ConnectionConfig, (v ?? 0) as never)}
                    placeholder="3001"
                    min={1}
                    max={65535}
                    error={error?.includes('端口') ? error : undefined}
                />
            </div>
            <TextField
                label="监听路径"
                value={data.path}
                onValueChange={(v) => patch('path' as keyof ConnectionConfig, v as never)}
                placeholder="/onebot/v11"
            />
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <Select
                    label="连接角色"
                    items={WS_ROLE_ITEMS}
                    value={data.role}
                    onValueChange={(v) => patch('role' as keyof ConnectionConfig, v as never)}
                />
                <NumberField
                    label="心跳间隔 (ms)"
                    required
                    value={data.heartInterval}
                    onValueChange={(v) =>
                        patch('heartInterval' as keyof ConnectionConfig, (v ?? 0) as never)
                    }
                    placeholder="30000"
                    min={1000}
                />
            </div>
            <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
                <Checkbox
                    label="上报 Bot 自身消息"
                    checked={data.reportSelfMessage}
                    onCheckedChange={(v) =>
                        patch('reportSelfMessage' as keyof ConnectionConfig, v as never)
                    }
                />
                <Checkbox
                    label="强制事件推送"
                    checked={data.enableForcePushEvent}
                    onCheckedChange={(v) =>
                        patch('enableForcePushEvent' as keyof ConnectionConfig, v as never)
                    }
                />
            </div>
        </>
    );
}

function WebsocketClientFields({
    data,
    patch,
    error,
}: {
    data: WebsocketClientConfig;
    patch: Patch;
    error: string | null;
}) {
    return (
        <>
            <TextField
                label="服务端 WebSocket URL"
                required
                value={data.url}
                onValueChange={(v) => patch('url' as keyof ConnectionConfig, v as never)}
                placeholder="ws://127.0.0.1:8080/onebot/v11"
                hint="必须以 ws:// 或 wss:// 开头"
                error={error?.includes('WebSocket URL') || error?.includes('URL') ? error : undefined}
            />
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                <Select
                    label="连接角色"
                    items={WS_ROLE_ITEMS}
                    value={data.role}
                    onValueChange={(v) => patch('role' as keyof ConnectionConfig, v as never)}
                    className="sm:col-span-3"
                />
                <NumberField
                    label="心跳间隔 (ms)"
                    required
                    value={data.heartInterval}
                    onValueChange={(v) =>
                        patch('heartInterval' as keyof ConnectionConfig, (v ?? 0) as never)
                    }
                    placeholder="30000"
                    min={1000}
                />
                <NumberField
                    label="重连间隔 (ms)"
                    required
                    value={data.reconnectInterval}
                    onValueChange={(v) =>
                        patch('reconnectInterval' as keyof ConnectionConfig, (v ?? 0) as never)
                    }
                    placeholder="5000"
                    min={1000}
                />
            </div>
            <Checkbox
                label="上报 Bot 自身发出的消息"
                checked={data.reportSelfMessage}
                onCheckedChange={(v) => patch('reportSelfMessage' as keyof ConnectionConfig, v as never)}
            />
        </>
    );
}
