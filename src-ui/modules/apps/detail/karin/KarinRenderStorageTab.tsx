// Karin「渲染与存储」：端点当表单写，不收进折叠卡。

import { Plus, Trash2 } from 'lucide-react';
import { Button, FormSection, NumberField, Switch, TextField } from '../../../../shared/ui';
import { RestartBadge } from '../FieldHints';
import { newRenderHttpServer, newRenderWsClient } from '../../../../core/domain/apps/karinConfig';
import { CONFIG_PAIR, ConfigForm } from './configLayout';
import type { KarinRedisConfig, KarinRenderConfig, KarinRenderHttpServer, KarinRenderWsClient } from '../../../../core/ipc/types';
import type { KarinTabProps } from './KarinBasicTab';

export const KarinRenderStorageTab: React.FC<KarinTabProps> = ({ config, onChange, errors, disabled }) => {
    const render = config.render;
    const redis = config.redis;
    const setRender = (patch: Partial<KarinRenderConfig>) =>
        onChange({ ...config, render: { ...render, ...patch } });
    const setRedis = (patch: Partial<KarinRedisConfig>) =>
        onChange({ ...config, redis: { ...redis, ...patch } });

    const patchWs = (idx: number, patch: Partial<KarinRenderWsClient>) =>
        setRender({
            ws_client: render.ws_client.map((row, i) => (i === idx ? { ...row, ...patch } : row)),
        });
    const patchHttp = (idx: number, patch: Partial<KarinRenderHttpServer>) =>
        setRender({
            http_server: render.http_server.map((row, i) => (i === idx ? { ...row, ...patch } : row)),
        });

    return (
        <ConfigForm>
            <FormSection
                title="渲染器 WS"
                actions={
                    <Switch
                        label="本机服务端"
                        checked={render.ws_server.enable}
                        disabled={disabled}
                        onCheckedChange={(enable) =>
                            setRender({ ws_server: { ...render.ws_server, enable } })
                        }
                    />
                }
                layout="none"
            >
                <div className="flex flex-col divide-y divide-border-subtle/70">
                    {render.ws_client.map((row, idx) => (
                        <div key={idx} className="flex flex-col gap-4 py-4 first:pt-0 last:pb-1">
                            <div className="flex flex-wrap items-center gap-x-8 gap-y-2">
                                <Switch
                                    label="启用"
                                    checked={row.enable}
                                    disabled={disabled}
                                    onCheckedChange={(enable) => patchWs(idx, { enable })}
                                />
                                <Switch
                                    label="Snapka"
                                    checked={row.isSnapka}
                                    disabled={disabled}
                                    onCheckedChange={(isSnapka) => patchWs(idx, { isSnapka })}
                                />
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    className="ml-auto h-7 w-7 text-danger hover:text-danger"
                                    aria-label="删除"
                                    disabled={disabled}
                                    onClick={() =>
                                        setRender({
                                            ws_client: render.ws_client.filter((_, i) => i !== idx),
                                        })
                                    }
                                >
                                    <Trash2 size={14} />
                                </Button>
                            </div>
                            <div className={CONFIG_PAIR}>
                                <TextField
                                    label="地址"
                                    error={errors[`render/ws_client/${idx}/url`]}
                                    placeholder="ws://127.0.0.1:7005"
                                    value={row.url}
                                    disabled={disabled}
                                    className="font-mono"
                                    onValueChange={(url) => patchWs(idx, { url })}
                                />
                                <TextField
                                    label="Token"
                                    value={row.token}
                                    disabled={disabled}
                                    className="font-mono"
                                    onValueChange={(token) => patchWs(idx, { token })}
                                />
                                <NumberField
                                    label="重连（ms）"
                                    value={row.reconnectTime ?? null}
                                    min={0}
                                    disabled={disabled}
                                    onValueChange={(v) => patchWs(idx, { reconnectTime: v ?? undefined })}
                                />
                                <NumberField
                                    label="心跳（ms）"
                                    value={row.heartbeatTime ?? null}
                                    min={0}
                                    disabled={disabled}
                                    onValueChange={(v) => patchWs(idx, { heartbeatTime: v ?? undefined })}
                                />
                            </div>
                        </div>
                    ))}
                </div>
                <div className="pt-3">
                    <Button
                        variant="secondary"
                        size="sm"
                        disabled={disabled}
                        onClick={() => setRender({ ws_client: [...render.ws_client, newRenderWsClient()] })}
                    >
                        <Plus size={13} /> 添加
                    </Button>
                </div>
            </FormSection>

            <FormSection title="渲染器 HTTP" layout="none">
                <div className="flex flex-col divide-y divide-border-subtle/70">
                    {render.http_server.map((row, idx) => (
                        <div key={idx} className="flex flex-col gap-4 py-4 first:pt-0 last:pb-1">
                            <div className="flex flex-wrap items-center gap-x-8 gap-y-2">
                                <Switch
                                    label="启用"
                                    checked={row.enable}
                                    disabled={disabled}
                                    onCheckedChange={(enable) => patchHttp(idx, { enable })}
                                />
                                <Switch
                                    label="Snapka"
                                    checked={row.isSnapka}
                                    disabled={disabled}
                                    onCheckedChange={(isSnapka) => patchHttp(idx, { isSnapka })}
                                />
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    className="ml-auto h-7 w-7 text-danger hover:text-danger"
                                    aria-label="删除"
                                    disabled={disabled}
                                    onClick={() =>
                                        setRender({
                                            http_server: render.http_server.filter((_, i) => i !== idx),
                                        })
                                    }
                                >
                                    <Trash2 size={14} />
                                </Button>
                            </div>
                            <div className={CONFIG_PAIR}>
                                <TextField
                                    label="地址"
                                    error={errors[`render/http_server/${idx}/url`]}
                                    placeholder="http://127.0.0.1:7005"
                                    value={row.url}
                                    disabled={disabled}
                                    className="font-mono"
                                    onValueChange={(url) => patchHttp(idx, { url })}
                                />
                                <TextField
                                    label="Token"
                                    value={row.token}
                                    disabled={disabled}
                                    className="font-mono"
                                    onValueChange={(token) => patchHttp(idx, { token })}
                                />
                            </div>
                        </div>
                    ))}
                </div>
                <div className="pt-3">
                    <Button
                        variant="secondary"
                        size="sm"
                        disabled={disabled}
                        onClick={() => setRender({ http_server: [...render.http_server, newRenderHttpServer()] })}
                    >
                        <Plus size={13} /> 添加
                    </Button>
                </div>
            </FormSection>

            <FormSection title="Redis" actions={<RestartBadge />}>
                <TextField
                    label="地址"
                    error={errors['redis/url']}
                    value={redis.url}
                    disabled={disabled}
                    className="font-mono"
                    onValueChange={(url) => setRedis({ url })}
                />
                <div className={CONFIG_PAIR}>
                    <TextField
                        label="用户名"
                        value={redis.username}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(username) => setRedis({ username })}
                    />
                    <TextField
                        label="密码"
                        type="password"
                        value={redis.password}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(password) => setRedis({ password })}
                    />
                    <NumberField
                        label="数据库"
                        value={redis.database}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => setRedis({ database: v ?? 0 })}
                    />
                </div>
            </FormSection>
        </ConfigForm>
    );
};
