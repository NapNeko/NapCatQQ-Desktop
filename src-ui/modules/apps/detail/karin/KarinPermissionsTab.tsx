// Karin「权限」：主人 / 管理员与名单两栏并排；事件范围用可展开的小卡片。

import { useState } from 'react';
import { FormSection, StringListField, Switch } from '../../../../shared/ui';
import { ExpandChevron, ExpandPresence } from '../../../../shared/ui/motion';
import { cn } from '../../../../shared/utils/cn';
import { CONFIG_PAIR, ConfigForm } from './configLayout';
import type { KarinCoreConfig, KarinEventScope } from '../../../../core/ipc/types';
import type { KarinTabProps } from './KarinBasicTab';

const SCOPES: ReadonlyArray<{
    key: 'friend' | 'group' | 'directs' | 'guilds' | 'channels';
    title: string;
}> = [
    { key: 'friend', title: '好友' },
    { key: 'group', title: '群' },
    { key: 'directs', title: '频道私信' },
    { key: 'guilds', title: '频道' },
    { key: 'channels', title: '子频道' },
];

export const KarinPermissionsTab: React.FC<KarinTabProps> = ({ config, onChange, disabled }) => {
    const core = config.config;
    const set = (patch: Partial<KarinCoreConfig>) => onChange({ ...config, config: { ...core, ...patch } });
    const setScope = (key: (typeof SCOPES)[number]['key'], patch: Partial<KarinEventScope>) =>
        set({ [key]: { ...core[key], ...patch } } as Partial<KarinCoreConfig>);

    const [open, setOpen] = useState<string | null>(null);

    return (
        <ConfigForm>
            <FormSection title="身份">
                <div className={CONFIG_PAIR}>
                    <StringListField
                        label="主人"
                        value={core.master}
                        disabled={disabled}
                        placeholder="QQ 号"
                        onChange={(master) => set({ master })}
                    />
                    <StringListField
                        label="管理员"
                        value={core.admin}
                        disabled={disabled}
                        placeholder="QQ 号"
                        onChange={(admin) => set({ admin })}
                    />
                </div>
            </FormSection>

            <FormSection title="用户名单">
                <div className={CONFIG_PAIR}>
                    <StringListField
                        label="白名单"
                        value={core.user.enable_list}
                        disabled={disabled}
                        onChange={(enable_list) => set({ user: { ...core.user, enable_list } })}
                    />
                    <StringListField
                        label="黑名单"
                        value={core.user.disable_list}
                        disabled={disabled}
                        onChange={(disable_list) => set({ user: { ...core.user, disable_list } })}
                    />
                </div>
            </FormSection>

            <FormSection title="事件范围" description="关掉即不接收该类事件；名单非空才限制">
                <div className="flex flex-col gap-2">
                    {SCOPES.map((s) => {
                        const scope = core[s.key];
                        const expanded = open === s.key;
                        return (
                            <div
                                key={s.key}
                                className="rounded-md border border-border-subtle/80 bg-field/40"
                            >
                                <div className="flex items-center gap-3 px-3 py-2">
                                    <button
                                        type="button"
                                        className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
                                        aria-expanded={expanded}
                                        onClick={() => setOpen(expanded ? null : s.key)}
                                    >
                                        <ExpandChevron open={expanded} />
                                        <span className="text-sm text-text">{s.title}</span>
                                    </button>
                                    <Switch
                                        checked={scope.enable}
                                        disabled={disabled}
                                        onCheckedChange={(enable) => setScope(s.key, { enable })}
                                    />
                                </div>
                                <ExpandPresence visible={expanded}>
                                    <div className={cn('border-t border-border-subtle/70 px-3 py-3', CONFIG_PAIR)}>
                                        <StringListField
                                            label="白名单"
                                            value={scope.enable_list}
                                            disabled={disabled || !scope.enable}
                                            onChange={(enable_list) => setScope(s.key, { enable_list })}
                                        />
                                        <StringListField
                                            label="黑名单"
                                            value={scope.disable_list}
                                            disabled={disabled || !scope.enable}
                                            onChange={(disable_list) => setScope(s.key, { disable_list })}
                                        />
                                        <StringListField
                                            label="日志白名单"
                                            value={scope.log_enable_list}
                                            disabled={disabled || !scope.enable}
                                            onChange={(log_enable_list) => setScope(s.key, { log_enable_list })}
                                        />
                                        <StringListField
                                            label="日志黑名单"
                                            value={scope.log_disable_list}
                                            disabled={disabled || !scope.enable}
                                            onChange={(log_disable_list) => setScope(s.key, { log_disable_list })}
                                        />
                                    </div>
                                </ExpandPresence>
                            </div>
                        );
                    })}
                </div>
            </FormSection>
        </ConfigForm>
    );
};
