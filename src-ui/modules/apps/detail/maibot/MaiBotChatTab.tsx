// 适配器的聊天名单：哪些群聊 / 私聊的消息交给麦麦。上游默认两边都是空白名单，
// 不改就一句都不回，所以这一页的提示比别处都直接。

import { AlertTriangle } from 'lucide-react';
import { FormSection, RadioGroup, StringListField, Switch } from '../../../../shared/ui';
import { ConfigForm } from '../karin/configLayout';
import { maibotChatDropsEverything } from '../../../../core/domain/apps/maibotConfig';
import type {
    MaiBotChatFilter,
    MaiBotInstanceConfig,
    MaiBotListMode,
} from '../../../../core/ipc/types';

const MODE_ITEMS: { value: MaiBotListMode; label: string }[] = [
    { value: 'whitelist', label: '只回名单里的' },
    { value: 'blacklist', label: '名单里的不回' },
];

export const MaiBotChatTab: React.FC<{
    config: MaiBotInstanceConfig;
    onChange: (next: MaiBotInstanceConfig) => void;
    errors: Record<string, string>;
    disabled?: boolean;
}> = ({ config, onChange, errors, disabled }) => {
    const adapter = config.adapter;
    if (!adapter) {
        return (
            <ConfigForm>
                <p className="text-sm text-text-secondary">
                    实例里没有 NapCat 适配器插件，名单无从设置。到实例菜单里重新安装可以补上。
                </p>
            </ConfigForm>
        );
    }
    const chat = adapter.chat;
    const set = (patch: Partial<MaiBotChatFilter>) =>
        onChange({ ...config, adapter: { ...adapter, chat: { ...chat, ...patch } } });
    const filtering = chat.enable_chat_list_filter;

    return (
        <ConfigForm>
            {maibotChatDropsEverything(chat) && (
                <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/5 px-3 py-2.5 text-[13px] leading-relaxed text-text">
                    <AlertTriangle size={15} className="mt-0.5 shrink-0 text-warning" />
                    <span>
                        群聊和私聊现在都是空的白名单，麦麦收到消息也会丢掉。加上要回复的群号，或者把群聊改成「名单里的不回」。
                    </span>
                </div>
            )}

            <FormSection title="名单过滤">
                <Switch
                    label="按名单过滤"
                    hint={filtering ? undefined : '关掉后群聊私聊都回，只看下面的屏蔽用户'}
                    checked={filtering}
                    disabled={disabled}
                    onCheckedChange={(v) => set({ enable_chat_list_filter: v })}
                />
            </FormSection>

            <FormSection title="群聊">
                <RadioGroup
                    orientation="horizontal"
                    items={MODE_ITEMS}
                    value={chat.group_list_type}
                    disabled={disabled || !filtering}
                    onValueChange={(group_list_type) => set({ group_list_type })}
                />
                <StringListField
                    label="群号"
                    value={chat.group_list}
                    error={errors['adapter/chat/group_list']}
                    disabled={disabled || !filtering}
                    placeholder="输入群号后回车"
                    onChange={(group_list) => set({ group_list })}
                />
            </FormSection>

            <FormSection title="私聊">
                <RadioGroup
                    orientation="horizontal"
                    items={MODE_ITEMS}
                    value={chat.private_list_type}
                    disabled={disabled || !filtering}
                    onValueChange={(private_list_type) => set({ private_list_type })}
                />
                <StringListField
                    label="QQ 号"
                    value={chat.private_list}
                    error={errors['adapter/chat/private_list']}
                    disabled={disabled || !filtering}
                    placeholder="输入 QQ 号后回车"
                    onChange={(private_list) => set({ private_list })}
                />
            </FormSection>

            <FormSection title="屏蔽用户">
                <StringListField
                    label="不管在哪都不回的 QQ 号"
                    value={chat.ban_user_id}
                    error={errors['adapter/chat/ban_user_id']}
                    disabled={disabled}
                    onChange={(ban_user_id) => set({ ban_user_id })}
                />
            </FormSection>
        </ConfigForm>
    );
};
