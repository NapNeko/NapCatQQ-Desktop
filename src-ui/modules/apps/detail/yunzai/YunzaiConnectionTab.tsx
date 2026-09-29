// 云崽「连接」：server.yaml（监听口 / 鉴权）和 redis.yaml。协议 Bot 反向 WS 连 /OneBotv11，
// 对接对话框会把口和 token 两边一起写好；这里改了端口或 token，已对接的会跟着重新对接。

import { FormSection, NumberField, TextField } from '../../../../shared/ui';
import { CopyCodeBlock } from '../../../../shared/ui/CopyCodeBlock';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import type { AppInstance, YunzaiRedisConfig, YunzaiServerConfig } from '../../../../core/ipc/types';
import type { YunzaiTabProps } from './YunzaiBasicTab';

export const YunzaiConnectionTab: React.FC<YunzaiTabProps & { instance: AppInstance }> = ({
    instance,
    config,
    onChange,
    errors,
    disabled,
}) => {
    const server = config.server;
    const redis = config.redis;
    const setServer = (patch: Partial<YunzaiServerConfig>) => onChange({ ...config, server: { ...server, ...patch } });
    const setRedis = (patch: Partial<YunzaiRedisConfig>) => onChange({ ...config, redis: { ...redis, ...patch } });
    const linked = !!instance.link;

    return (
        <ConfigForm>
            <FormSection title="对接协议 Bot">
                <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm">
                    <dt className="text-text-tertiary">对接</dt>
                    <dd className="text-text">
                        {linked ? `已对接 Bot ${instance.link?.bot_id}` : '还没对接，到「概览」点对接'}
                    </dd>
                    <dt className="text-text-tertiary">鉴权</dt>
                    <dd className="text-text">{server.access_token ? '要 token（Authorization: Bearer）' : '不鉴权'}</dd>
                </dl>
                <div className="flex flex-col gap-1.5">
                    <span className="text-xs text-text-secondary">反向 WS 地址（别的协议端手动接也填这个）</span>
                    <CopyCodeBlock command={`ws://127.0.0.1:${server.port}/OneBotv11`} />
                </div>
                {server.extra_auth_headers.length > 0 && (
                    <p className="text-xs text-warning">
                        auth 里还有 {server.extra_auth_headers.join('、')}：上游要求每一条都对上，NapCat 只带 Authorization，
                        再点一次对接会把它们去掉
                    </p>
                )}
            </FormSection>

            <FormSection title="云崽服务" description="HTTP 和 WS 共用一个口，监听全部网卡；改端口要重启">
                <div className={CONFIG_PAIR}>
                    <NumberField
                        label="端口"
                        value={server.port}
                        min={1}
                        max={65535}
                        error={errors['server/port']}
                        disabled={disabled}
                        onValueChange={(v) => setServer({ port: v ?? server.port })}
                    />
                    <TextField
                        label="token"
                        hint="留空不鉴权；对接时会自动生成"
                        value={server.access_token}
                        error={errors['server/access_token']}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(access_token) => setServer({ access_token: access_token.trim() })}
                    />
                    <TextField
                        label="对外地址"
                        hint="发给协议端的文件链接用它拼；端口跟着上面走"
                        value={server.url}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(url) => setServer({ url })}
                    />
                    <TextField
                        label="访问不存在的路径时跳到"
                        value={server.redirect}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(redirect) => setServer({ redirect })}
                    />
                </div>
            </FormSection>

            <FormSection title="Redis" description="连不上且地址是 127.0.0.1 时云崽自己拉起一个；改了要重启">
                <div className={CONFIG_PAIR}>
                    <TextField
                        label="地址"
                        value={redis.host}
                        error={errors['redis/host']}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(host) => setRedis({ host })}
                    />
                    <NumberField
                        label="端口"
                        value={redis.port}
                        min={1}
                        max={65535}
                        error={errors['redis/port']}
                        disabled={disabled}
                        onValueChange={(v) => setRedis({ port: v ?? redis.port })}
                    />
                    <NumberField
                        label="库号"
                        value={redis.db}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => setRedis({ db: v ?? 0 })}
                    />
                    <TextField
                        label="密码"
                        type="password"
                        value={redis.password}
                        disabled={disabled}
                        onValueChange={(password) => setRedis({ password })}
                    />
                </div>
                <TextField
                    label="redis-server 路径"
                    hint="桌面端装的 Redis 在组件页"
                    value={redis.path}
                    error={errors['redis/path']}
                    disabled={disabled}
                    className="font-mono"
                    onValueChange={(path) => setRedis({ path })}
                />
            </FormSection>
        </ConfigForm>
    );
};
