// 连接：两个监听口可改；和协议 Bot 的对接只读（由对接对话框写，方向是麦麦去连 Bot）。

import { FormSection, NumberField } from '../../../../shared/ui';
import { CopyCodeBlock } from '../../../../shared/ui/CopyCodeBlock';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import type { AppInstance, MaiBotInstanceConfig } from '../../../../core/ipc/types';

export const MaiBotConnectionTab: React.FC<{
    instance: AppInstance;
    config: MaiBotInstanceConfig;
    onChange: (next: MaiBotInstanceConfig) => void;
    errors: Record<string, string>;
    disabled?: boolean;
}> = ({ instance, config, onChange, errors, disabled }) => {
    const adapter = config.adapter;
    const linked = !!instance.link;

    return (
        <ConfigForm>
            <FormSection title="对接协议 Bot">
                {!adapter ? (
                    <p className="text-sm text-text-secondary">
                        实例里没有 NapCat 适配器插件，重新安装实例可以补上。
                    </p>
                ) : (
                    <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm">
                        <dt className="text-text-tertiary">对接</dt>
                        <dd className="text-text">
                            {linked ? `已对接 Bot ${instance.link?.bot_id}` : '还没对接，到「概览」点对接'}
                        </dd>
                        <dt className="text-text-tertiary">适配器</dt>
                        <dd className="text-text">{adapter.enabled ? '已启用' : '未启用'}</dd>
                        <dt className="text-text-tertiary">连接地址</dt>
                        {/* 没对接时适配器里是上游默认的 3001，Bot 并没在那儿听，显示出来只会误导 */}
                        {adapter.enabled || linked ? (
                            <dd className="font-mono text-text">
                                ws://{adapter.napcat_host}:{adapter.napcat_port}
                                <span className="ml-2 font-sans text-2xs text-text-tertiary">
                                    {adapter.has_token ? '带 token' : '没设 token'}
                                </span>
                            </dd>
                        ) : (
                            <dd className="text-text-secondary">对接时自动填好</dd>
                        )}
                    </dl>
                )}
            </FormSection>

            <FormSection title="WebUI">
                <div className={CONFIG_PAIR}>
                    <NumberField
                        label="WebUI 端口"
                        value={config.bot.webui.port}
                        min={1}
                        max={65535}
                        error={errors['bot/webui/port']}
                        disabled={disabled}
                        onValueChange={(v) =>
                            onChange({
                                ...config,
                                bot: { ...config.bot, webui: { ...config.bot.webui, port: v ?? config.bot.webui.port } },
                            })
                        }
                    />
                    <NumberField
                        label="旧版消息服务端口"
                        value={config.bot.maim_message.ws_server_port}
                        min={1}
                        max={65535}
                        error={errors['bot/maim_message/ws_server_port']}
                        hint="适配器插件用不上，但麦麦启动时总会监听，被占用就起不来"
                        disabled={disabled}
                        onValueChange={(v) =>
                            onChange({
                                ...config,
                                bot: {
                                    ...config.bot,
                                    maim_message: {
                                        ...config.bot.maim_message,
                                        ws_server_port: v ?? config.bot.maim_message.ws_server_port,
                                    },
                                },
                            })
                        }
                    />
                </div>
                {config.webui_token && (
                    <div className="flex flex-col gap-1.5">
                        <span className="text-xs text-text-secondary">登录 token（改 token 去 WebUI 的设置里）</span>
                        <CopyCodeBlock command={config.webui_token} />
                    </div>
                )}
            </FormSection>
        </ConfigForm>
    );
};
