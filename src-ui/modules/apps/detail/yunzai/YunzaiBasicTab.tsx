// 云崽「基础」：bot.yaml 里进程怎么跑的那部分。渲染用的浏览器在「渲染」页，端口在「连接」页。

import { FormSection, NumberField, Select, StringListField, Switch, TextField } from '../../../../shared/ui';
import { YUNZAI_LOG_LEVELS } from '../../../../core/domain/apps/yunzaiConfig';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import type { YunzaiBotConfig, YunzaiInstanceConfig } from '../../../../core/ipc/types';

export interface YunzaiTabProps {
    config: YunzaiInstanceConfig;
    onChange: (next: YunzaiInstanceConfig) => void;
    errors: Record<string, string>;
    disabled?: boolean;
}

const LOG_LEVEL_ITEMS = YUNZAI_LOG_LEVELS.map((v) => ({ value: v, label: v }));

export const YunzaiBasicTab: React.FC<YunzaiTabProps> = ({ config, onChange, errors, disabled }) => {
    const bot = config.bot;
    const set = (patch: Partial<YunzaiBotConfig>) => onChange({ ...config, bot: { ...bot, ...patch } });

    return (
        <ConfigForm>
            <FormSection title="日志">
                <div className={CONFIG_PAIR}>
                    <Select
                        label="等级"
                        error={errors['bot/log_level']}
                        items={LOG_LEVEL_ITEMS}
                        value={bot.log_level}
                        disabled={disabled}
                        onValueChange={(log_level) => set({ log_level })}
                    />
                    <NumberField
                        label="单条最长字数"
                        value={bot.log_length}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => set({ log_length: v ?? 0 })}
                    />
                </div>
                <Switch
                    label="对象按结构展开"
                    checked={bot.log_object}
                    disabled={disabled}
                    onCheckedChange={(log_object) => set({ log_object })}
                />
            </FormSection>

            <FormSection title="自动更新与重启" description="改了要重启云崽才重新排">
                <div className={CONFIG_PAIR}>
                    <NumberField
                        label="每隔多少分钟自动更新"
                        hint="更新本体和全部插件，0 不自动更新"
                        value={bot.update_time}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => set({ update_time: v ?? 0 })}
                    />
                    <NumberField
                        label="每隔多少分钟自动重启"
                        hint="0 不自动重启"
                        value={bot.restart_time}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => set({ restart_time: v ?? 0 })}
                    />
                    <StringListField
                        label="定时更新（cron）"
                        value={bot.update_cron}
                        disabled={disabled}
                        placeholder="0 4 * * *"
                        onChange={(update_cron) => set({ update_cron })}
                    />
                    <StringListField
                        label="定时重启（cron）"
                        value={bot.restart_cron}
                        disabled={disabled}
                        placeholder="0 5 * * *"
                        onChange={(restart_cron) => set({ restart_cron })}
                    />
                    <StringListField
                        label="定时关机（cron）"
                        value={bot.stop_cron}
                        disabled={disabled}
                        onChange={(stop_cron) => set({ stop_cron })}
                    />
                    <StringListField
                        label="定时开机（cron）"
                        value={bot.start_cron}
                        disabled={disabled}
                        onChange={(start_cron) => set({ start_cron })}
                    />
                </div>
            </FormSection>

            <FormSection title="消息">
                <div className="flex flex-wrap gap-x-8 gap-y-3">
                    <Switch
                        label="/ 开头当作 #"
                        checked={bot.slash_to_hash}
                        disabled={disabled}
                        onCheckedChange={(slash_to_hash) => set({ slash_to_hash })}
                    />
                    <Switch
                        label="缓存群成员列表"
                        checked={bot.cache_group_member}
                        disabled={disabled}
                        onCheckedChange={(cache_group_member) => set({ cache_group_member })}
                    />
                </div>
                <div className={CONFIG_PAIR}>
                    <NumberField
                        label="上线通知冷却（分钟）"
                        value={bot.online_msg_exp}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => set({ online_msg_exp: v ?? 0 })}
                    />
                    <NumberField
                        label="文件链接保留（分钟）"
                        value={bot.file_to_url_time}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => set({ file_to_url_time: v ?? 0 })}
                    />
                </div>
            </FormSection>

            <FormSection title="进阶">
                <Switch
                    label="监听文件变化"
                    hint="关掉后改配置、改单 JS 插件都要重启才生效"
                    checked={bot.file_watch}
                    disabled={disabled}
                    onCheckedChange={(file_watch) => set({ file_watch })}
                />
                <div className={CONFIG_PAIR}>
                    <NumberField
                        label="插件加载超时（秒）"
                        value={bot.plugin_load_timeout}
                        min={1}
                        disabled={disabled}
                        onValueChange={(v) => set({ plugin_load_timeout: v ?? 60 })}
                    />
                    <TextField
                        label="米游社接口代理"
                        hint="国际服用，留空不走代理"
                        value={bot.proxy_address}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(proxy_address) => set({ proxy_address })}
                    />
                </div>
            </FormSection>
        </ConfigForm>
    );
};
