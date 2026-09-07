// Karin 单条连接的 Dialog 表单，交互对齐 Bot ConnectionEditor：草稿编辑，确定后写回配置。

import { useEffect, useState } from 'react';
import { Button, NumberField, Switch, TextField } from '../../../../shared/ui';
import { FieldLabel, LinkDependentBadge, PortSyncBadge } from '../FieldHints';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import type {
    KarinConsoleAdapter,
    KarinEnv,
    KarinOneBotHttpServer,
    KarinOneBotWsClient,
    KarinOneBotWsServer,
} from '../../../../core/ipc/types';
import type { KarinConnKind } from './karinConnectionsModel';

const isWs = (u: string) => /^wss?:\/\//.test(u.trim());
const isHttp = (u: string) => /^https?:\/\//.test(u.trim());

export type KarinConnDraft =
    | {
          kind: 'webui';
          env: Pick<KarinEnv, 'http_enable' | 'http_port' | 'http_host' | 'http_auth_key'>;
      }
    | {
          kind: 'reverseWs';
          envKey: string;
          ws_server: KarinOneBotWsServer;
      }
    | { kind: 'forwardWs'; row: KarinOneBotWsClient }
    | { kind: 'onebotHttp'; row: KarinOneBotHttpServer }
    | { kind: 'console'; console: KarinConsoleAdapter };

export const KarinConnectionEditor: React.FC<{
    draft: KarinConnDraft;
    linked: boolean;
    fieldErrors: Record<string, string>;
    onSave: (next: KarinConnDraft) => void;
    onCancel: () => void;
}> = ({ draft, linked, fieldErrors, onSave, onCancel }) => {
    const [data, setData] = useState(draft);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        setData(draft);
        setError(null);
    }, [draft]);

    const handleSave = () => {
        const next = trimDraft(data);
        const reason = validateDraft(next);
        if (reason) {
            setError(reason);
            pushInfoBar({
                tone: 'danger',
                title: '连接配置有误',
                content: reason,
                autoDismissMs: 4000,
            });
            return;
        }
        onSave(next);
    };

    return (
        <div className="flex flex-col gap-3">
            <DraftFields
                data={data}
                setData={setData}
                linked={linked}
                fieldErrors={fieldErrors}
                error={error}
            />
            <div className="mt-3 flex items-center justify-between gap-3 border-t border-border-subtle pt-4">
                <DraftEnable data={data} setData={setData} />
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
};

function DraftEnable({
    data,
    setData,
}: {
    data: KarinConnDraft;
    setData: (next: KarinConnDraft) => void;
}) {
    if (data.kind === 'console') {
        return (
            <Switch
                checked={data.console.isLocal}
                onCheckedChange={(isLocal) => setData({ ...data, console: { ...data.console, isLocal } })}
                label="仅本机"
            />
        );
    }
    if (data.kind === 'webui') {
        return (
            <Switch
                checked={data.env.http_enable}
                onCheckedChange={(http_enable) => setData({ ...data, env: { ...data.env, http_enable } })}
                label="启用此连接"
            />
        );
    }
    if (data.kind === 'reverseWs') {
        return (
            <Switch
                checked={data.ws_server.enable}
                onCheckedChange={(enable) =>
                    setData({ ...data, ws_server: { ...data.ws_server, enable } })
                }
                label="启用此连接"
            />
        );
    }
    if (data.kind === 'forwardWs') {
        return (
            <Switch
                checked={data.row.enable}
                onCheckedChange={(enable) => setData({ ...data, row: { ...data.row, enable } })}
                label="启用此连接"
            />
        );
    }
    return (
        <Switch
            checked={data.row.enable}
            onCheckedChange={(enable) => setData({ ...data, row: { ...data.row, enable } })}
            label="启用此连接"
        />
    );
}

function DraftFields({
    data,
    setData,
    linked,
    fieldErrors,
    error,
}: {
    data: KarinConnDraft;
    setData: (next: KarinConnDraft) => void;
    linked: boolean;
    fieldErrors: Record<string, string>;
    error: string | null;
}) {
    switch (data.kind) {
        case 'webui':
            return (
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    <NumberField
                        label={
                            <FieldLabel text="端口">
                                <PortSyncBadge />
                            </FieldLabel>
                        }
                        error={pickErr(error, fieldErrors['env/http_port'], '端口')}
                        value={data.env.http_port}
                        min={1}
                        max={65535}
                        onValueChange={(v) =>
                            setData({ ...data, env: { ...data.env, http_port: v ?? 0 } })
                        }
                    />
                    <TextField
                        label="监听地址"
                        value={data.env.http_host}
                        className="font-mono"
                        onValueChange={(http_host) =>
                            setData({ ...data, env: { ...data.env, http_host } })
                        }
                    />
                    <TextField
                        label="HTTP 秘钥"
                        value={data.env.http_auth_key}
                        className="font-mono sm:col-span-2"
                        onValueChange={(http_auth_key) =>
                            setData({ ...data, env: { ...data.env, http_auth_key } })
                        }
                    />
                </div>
            );
        case 'reverseWs':
            return (
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    <TextField
                        label={
                            <FieldLabel text="鉴权秘钥">
                                <LinkDependentBadge linked={linked} />
                            </FieldLabel>
                        }
                        value={data.envKey}
                        className="font-mono"
                        onValueChange={(envKey) => setData({ ...data, envKey })}
                    />
                    <NumberField
                        label="鉴权超时（秒）"
                        error={pickErr(error, fieldErrors['adapter/onebot/ws_server/timeout'], '超时')}
                        value={data.ws_server.timeout}
                        min={1}
                        onValueChange={(v) =>
                            setData({
                                ...data,
                                ws_server: { ...data.ws_server, timeout: v ?? 0 },
                            })
                        }
                    />
                </div>
            );
        case 'forwardWs':
            return (
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    <TextField
                        label="地址"
                        value={data.row.url}
                        error={pickErr(error, undefined, 'ws://')}
                        className="font-mono sm:col-span-2"
                        placeholder="ws://127.0.0.1:3001"
                        onValueChange={(url) => setData({ ...data, row: { ...data.row, url } })}
                    />
                    <TextField
                        label="Token"
                        value={data.row.token}
                        className="font-mono sm:col-span-2"
                        onValueChange={(token) => setData({ ...data, row: { ...data.row, token } })}
                    />
                </div>
            );
        case 'onebotHttp':
            return (
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    <TextField
                        label="地址"
                        value={data.row.url}
                        error={pickErr(error, undefined, 'http')}
                        className="font-mono sm:col-span-2"
                        placeholder="http://127.0.0.1:3000"
                        onValueChange={(url) => setData({ ...data, row: { ...data.row, url } })}
                    />
                    <TextField
                        label="self_id"
                        value={data.row.self_id}
                        error={pickErr(error, undefined, 'self_id')}
                        className="font-mono"
                        onValueChange={(self_id) => setData({ ...data, row: { ...data.row, self_id } })}
                    />
                    <TextField
                        label="Token"
                        value={data.row.token}
                        className="font-mono"
                        onValueChange={(token) => setData({ ...data, row: { ...data.row, token } })}
                    />
                    <TextField
                        label="api_token"
                        value={data.row.api_token}
                        className="font-mono"
                        onValueChange={(api_token) =>
                            setData({ ...data, row: { ...data.row, api_token } })
                        }
                    />
                    <TextField
                        label="post_token"
                        value={data.row.post_token}
                        className="font-mono"
                        onValueChange={(post_token) =>
                            setData({ ...data, row: { ...data.row, post_token } })
                        }
                    />
                </div>
            );
        case 'console':
            return (
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    <TextField
                        label="Token"
                        value={data.console.token}
                        className="font-mono"
                        onValueChange={(token) =>
                            setData({ ...data, console: { ...data.console, token } })
                        }
                    />
                    <TextField
                        label="Host"
                        value={data.console.host}
                        className="font-mono"
                        onValueChange={(host) =>
                            setData({ ...data, console: { ...data.console, host } })
                        }
                    />
                </div>
            );
    }
}

export function kindTitle(kind: KarinConnKind): string {
    switch (kind) {
        case 'webui':
            return 'WebUI';
        case 'reverseWs':
            return '反向 WS';
        case 'forwardWs':
            return '正向 WS';
        case 'onebotHttp':
            return 'OneBot HTTP';
        case 'console':
            return '控制台';
    }
}

function trimDraft(data: KarinConnDraft): KarinConnDraft {
    if (data.kind === 'webui') {
        return {
            ...data,
            env: {
                ...data.env,
                http_host: data.env.http_host.trim(),
                http_auth_key: data.env.http_auth_key.trim(),
            },
        };
    }
    if (data.kind === 'reverseWs') {
        return { ...data, envKey: data.envKey.trim() };
    }
    if (data.kind === 'forwardWs') {
        return { ...data, row: { ...data.row, url: data.row.url.trim(), token: data.row.token.trim() } };
    }
    if (data.kind === 'onebotHttp') {
        return {
            ...data,
            row: {
                ...data.row,
                url: data.row.url.trim(),
                self_id: data.row.self_id.trim(),
                token: data.row.token.trim(),
                api_token: data.row.api_token.trim(),
                post_token: data.row.post_token.trim(),
            },
        };
    }
    return {
        ...data,
        console: {
            ...data.console,
            token: data.console.token.trim(),
            host: data.console.host.trim(),
        },
    };
}

function validateDraft(data: KarinConnDraft): string | null {
    if (data.kind === 'webui') {
        if (!Number.isInteger(data.env.http_port) || data.env.http_port < 1 || data.env.http_port > 65535) {
            return '端口需在 1–65535';
        }
        return null;
    }
    if (data.kind === 'reverseWs') {
        if (!(data.ws_server.timeout > 0)) return '超时须大于 0 秒';
        return null;
    }
    if (data.kind === 'forwardWs') {
        if (!isWs(data.row.url)) return '须以 ws:// 或 wss:// 开头';
        return null;
    }
    if (data.kind === 'onebotHttp') {
        if (!isHttp(data.row.url)) return '须以 http:// 或 https:// 开头';
        if (!data.row.self_id.trim()) return 'self_id 不能为空';
        return null;
    }
    return null;
}

function pickErr(local: string | null, field: string | undefined, needle: string): string | undefined {
    if (local && local.includes(needle)) return local;
    return field;
}
