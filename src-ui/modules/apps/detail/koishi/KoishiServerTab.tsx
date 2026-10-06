// 服务器：server 插件的端口 / 监听地址 / 对外地址。端口同时是控制台口和 Bot 反向连过来的口，
// 改了桌面端会跟着改实例端口、重写 Bot 侧的对接地址。

import { AlertTriangle, SquareArrowOutUpRight } from 'lucide-react';
import { Button, FormSection, NumberField, Select, TextField } from '../../../../shared/ui';
import { CopyCodeBlock } from '../../../../shared/ui/CopyCodeBlock';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import { koishiServer, setServerField } from '../../../../core/domain/apps/koishiConfig';
import type { KoishiInstanceConfig } from '../../../../core/ipc/types';

const HOSTS = [
    { value: '127.0.0.1', label: '只本机（127.0.0.1）' },
    { value: '0.0.0.0', label: '所有网卡（0.0.0.0）' },
];

export const KoishiServerTab: React.FC<{
    config: KoishiInstanceConfig;
    onChange: (next: KoishiInstanceConfig) => void;
    errors: Record<string, string>;
    disabled?: boolean;
    running: boolean;
    /** 在外部浏览器打开上游控制台（沙盒 / 数据库这些在应用内的「工具」组有原生页） */
    onOpenWebUi: () => void;
}> = ({ config, onChange, errors, disabled, running, onOpenWebUi }) => {
    const server = koishiServer(config);
    const hostKnown = HOSTS.some((h) => h.value === server.host);
    const open = server.host !== '127.0.0.1' && server.host !== 'localhost';
    return (
        <ConfigForm>
            <FormSection
                title="监听"
                description="控制台和 Bot 反向连接共用这一个口；改端口后桌面端会同步改 Bot 侧的对接地址"
            >
                <div className={CONFIG_PAIR}>
                    <NumberField
                        label="端口"
                        value={server.port}
                        min={1}
                        max={65535}
                        error={errors['server/port']}
                        disabled={disabled}
                        onValueChange={(v) =>
                            v !== null && onChange(setServerField(config, 'port', v))
                        }
                    />
                    <Select
                        label="监听地址"
                        value={hostKnown ? server.host : '__custom'}
                        items={
                            hostKnown
                                ? HOSTS
                                : [...HOSTS, { value: '__custom', label: server.host }]
                        }
                        hint="跨机器对接桌面端会走 SSH 隧道，不用对外开放"
                        disabled={disabled}
                        onValueChange={(v) =>
                            v !== '__custom' && onChange(setServerField(config, 'host', v))
                        }
                    />
                </div>
                {open && (
                    <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/5 px-3 py-2.5 text-[13px] leading-relaxed text-text">
                        <AlertTriangle size={15} className="mt-0.5 shrink-0 text-warning" />
                        <span>
                            局域网、公网都能打开控制台，而它默认不要登录、能装插件改配置。开放前先到「插件」页启用
                            auth 插件设好账号
                        </span>
                    </div>
                )}
            </FormSection>

            <FormSection title="对外地址">
                <TextField
                    label="selfUrl"
                    value={server.selfUrl}
                    placeholder="例如 https://bot.example.com"
                    hint="有反向代理或域名时填，发图片、网页回调这类插件拼链接要用；留空用本机地址"
                    disabled={disabled}
                    onValueChange={(v) => onChange(setServerField(config, 'selfUrl', v.trim()))}
                />
            </FormSection>

            <FormSection
                title="控制台"
                description="上游网页控制台；沙盒、指令、数据库、文件在应用内的「工具」组里有原生页"
                actions={
                    <Button size="sm" variant="secondary" disabled={!running} onClick={onOpenWebUi}>
                        <SquareArrowOutUpRight size={13} />
                        {running ? '在浏览器打开' : '启动后可打开'}
                    </Button>
                }
            >
                <div className="flex flex-col gap-1.5">
                    <span className="text-xs text-text-secondary">
                        本机地址；远端实例由桌面端经 SSH 隧道打开
                    </span>
                    <CopyCodeBlock command={`http://127.0.0.1:${server.port}/`} />
                </div>
            </FormSection>
        </ConfigForm>
    );
};
