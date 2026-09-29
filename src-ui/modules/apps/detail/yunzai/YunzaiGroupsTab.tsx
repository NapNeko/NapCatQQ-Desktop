// 云崽「群聊」：group.yaml。上面是所有群的默认，下面是单独设置（群号 / Bot号:default / Bot号:群号），
// 单独设置里留空的项沿用默认。添加消息那几项只放默认里：单独设置里改它们的人很少，要改走原始文件。

import { useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { Badge, Button, FormSection, NumberField, Select, StringListField, Switch, TextField } from '../../../../shared/ui';
import { ExpandChevron, ExpandPresence } from '../../../../shared/ui/motion';
import {
    YUNZAI_ADD_LIMITS,
    YUNZAI_REPLY_MODES,
    newYunzaiGroupOverride,
    yunzaiOverrideFieldCount,
    yunzaiOverrideScope,
} from '../../../../core/domain/apps/yunzaiConfig';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import type { YunzaiGroupDefaults, YunzaiGroupOverride } from '../../../../core/ipc/types';
import type { YunzaiTabProps } from './YunzaiBasicTab';

const REPLY_ITEMS = YUNZAI_REPLY_MODES.map((m) => ({ value: String(m.value), label: m.label }));
const ADD_LIMIT_ITEMS = YUNZAI_ADD_LIMITS.map((m) => ({ value: String(m.value), label: m.label }));
const INHERIT = '__inherit__';

export const YunzaiGroupsTab: React.FC<YunzaiTabProps> = ({ config, onChange, errors, disabled }) => {
    const group = config.group;
    const d = group.default;
    const setDefault = (patch: Partial<YunzaiGroupDefaults>) =>
        onChange({ ...config, group: { ...group, default: { ...d, ...patch } } });
    const setOverrides = (overrides: YunzaiGroupOverride[]) => onChange({ ...config, group: { ...group, overrides } });
    // 新加的那条直接展开
    const [openIndex, setOpenIndex] = useState<number | null>(null);

    return (
        <ConfigForm>
            <FormSection title="所有群的默认">
                <Select
                    label="响应哪些消息"
                    error={errors['group/default/only_reply_at']}
                    items={REPLY_ITEMS}
                    value={String(d.only_reply_at)}
                    disabled={disabled}
                    onValueChange={(v) => setDefault({ only_reply_at: Number(v) })}
                />
                <StringListField
                    label="别名"
                    hint="消息以别名开头也算叫到了 Bot"
                    value={d.bot_alias}
                    mono={false}
                    disabled={disabled}
                    onChange={(bot_alias) => setDefault({ bot_alias })}
                />
                <div className={CONFIG_PAIR}>
                    <NumberField
                        label="群冷却（毫秒）"
                        hint="群里任何指令之间，0 不限"
                        value={d.group_cd}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => setDefault({ group_cd: v ?? 0 })}
                    />
                    <NumberField
                        label="个人冷却（毫秒）"
                        value={d.single_cd}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => setDefault({ single_cd: v ?? 0 })}
                    />
                    <StringListField
                        label="只启用这些功能"
                        hint="填了就只有这些响应"
                        value={d.enable}
                        mono={false}
                        disabled={disabled}
                        onChange={(enable) => setDefault({ enable })}
                    />
                    <StringListField
                        label="禁用功能"
                        hint="功能名，比如 十连、角色查询"
                        value={d.disable}
                        mono={false}
                        disabled={disabled}
                        onChange={(disable) => setDefault({ disable })}
                    />
                </div>
            </FormSection>

            <FormSection title="添加消息（#添加）">
                <div className={CONFIG_PAIR}>
                    <Select
                        label="谁能添加"
                        error={errors['group/default/add_limit']}
                        items={ADD_LIMIT_ITEMS}
                        value={String(d.add_limit)}
                        disabled={disabled}
                        onValueChange={(v) => setDefault({ add_limit: Number(v) })}
                    />
                    <NumberField
                        label="回复多少秒后撤回"
                        hint="0 不撤回"
                        value={d.add_recall}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => setDefault({ add_recall: v ?? 0 })}
                    />
                </div>
                <div className="flex flex-wrap gap-x-8 gap-y-3">
                    <Switch
                        label="允许私聊添加"
                        checked={d.add_private === 1}
                        disabled={disabled}
                        onCheckedChange={(on) => setDefault({ add_private: on ? 1 : 0 })}
                    />
                    <Switch
                        label="回复时引用触发消息"
                        checked={d.add_reply === 1}
                        disabled={disabled}
                        onCheckedChange={(on) => setDefault({ add_reply: on ? 1 : 0 })}
                    />
                    <Switch
                        label="回复时 @ 触发的人"
                        checked={d.add_at === 1}
                        disabled={disabled}
                        onCheckedChange={(on) => setDefault({ add_at: on ? 1 : 0 })}
                    />
                </div>
            </FormSection>

            <FormSection
                title="单独设置"
                description="优先级：Bot号:群号 → 群号 → Bot号:default → 默认"
                actions={
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={disabled}
                        onClick={() => {
                            setOverrides([...group.overrides, newYunzaiGroupOverride()]);
                            setOpenIndex(group.overrides.length);
                        }}
                    >
                        <Plus size={13} /> 添加
                    </Button>
                }
                layout="none"
            >
                {group.overrides.length === 0 && (
                    <p className="text-sm text-text-tertiary">还没有，所有群都按上面的默认</p>
                )}
                {group.overrides.map((o, i) => (
                    <OverrideCard
                        key={i}
                        rule={o}
                        index={i}
                        defaults={d}
                        errors={errors}
                        disabled={disabled}
                        defaultOpen={openIndex === i}
                        onChange={(next) => setOverrides(group.overrides.map((r, j) => (j === i ? next : r)))}
                        onRemove={() => setOverrides(group.overrides.filter((_, j) => j !== i))}
                    />
                ))}
            </FormSection>
        </ConfigForm>
    );
};

const OverrideCard: React.FC<{
    rule: YunzaiGroupOverride;
    index: number;
    defaults: YunzaiGroupDefaults;
    errors: Record<string, string>;
    disabled?: boolean;
    defaultOpen: boolean;
    onChange: (next: YunzaiGroupOverride) => void;
    onRemove: () => void;
}> = ({ rule, index, defaults, errors, disabled, defaultOpen, onChange, onRemove }) => {
    const [open, setOpen] = useState(defaultOpen);
    const set = (patch: Partial<YunzaiGroupOverride>) => onChange({ ...rule, ...patch });
    const keyError = errors[`group/overrides/${index}/key`];
    const count = yunzaiOverrideFieldCount(rule);
    const replyLabel = YUNZAI_REPLY_MODES.find((m) => m.value === defaults.only_reply_at)?.label ?? '';

    return (
        <div className="border-b border-border-subtle/70 py-3 first:pt-1 last:border-0 last:pb-1">
            <div className="flex items-center gap-2">
                <button
                    type="button"
                    className="flex min-w-0 flex-1 items-center gap-2 text-left"
                    aria-expanded={open}
                    onClick={() => setOpen((v) => !v)}
                >
                    <ExpandChevron open={open} />
                    <span className="truncate font-mono text-sm text-text">{rule.key || '（未填）'}</span>
                    <span className="truncate text-2xs text-text-tertiary">{yunzaiOverrideScope(rule.key)}</span>
                    {count > 0 && (
                        <Badge tone="neutral" appearance="outline">
                            改了 {count} 项
                        </Badge>
                    )}
                    {keyError && <span className="text-2xs text-danger">{keyError}</span>}
                </button>
                <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-danger hover:text-danger"
                    aria-label="删除单独设置"
                    disabled={disabled}
                    onClick={onRemove}
                >
                    <Trash2 size={14} />
                </Button>
            </div>

            <ExpandPresence visible={open}>
                <div className="flex flex-col gap-5 pt-4">
                    <TextField
                        label="对谁生效"
                        hint="群号、Bot号:default 或 Bot号:群号"
                        error={keyError}
                        value={rule.key}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(key) => set({ key })}
                    />
                    <Select
                        label="响应哪些消息"
                        items={[{ value: INHERIT, label: `沿用默认（${replyLabel}）` }, ...REPLY_ITEMS]}
                        value={rule.only_reply_at == null ? INHERIT : String(rule.only_reply_at)}
                        disabled={disabled}
                        onValueChange={(v) => set({ only_reply_at: v === INHERIT ? undefined : Number(v) })}
                    />
                    <div className={CONFIG_PAIR}>
                        <NumberField
                            label="群冷却（毫秒）"
                            placeholder={`沿用默认 ${defaults.group_cd}`}
                            value={rule.group_cd ?? null}
                            min={0}
                            disabled={disabled}
                            onValueChange={(v) => set({ group_cd: v ?? undefined })}
                        />
                        <NumberField
                            label="个人冷却（毫秒）"
                            placeholder={`沿用默认 ${defaults.single_cd}`}
                            value={rule.single_cd ?? null}
                            min={0}
                            disabled={disabled}
                            onValueChange={(v) => set({ single_cd: v ?? undefined })}
                        />
                    </div>
                    <InheritableList
                        label="别名"
                        value={rule.bot_alias}
                        fallback={defaults.bot_alias}
                        disabled={disabled}
                        onChange={(bot_alias) => set({ bot_alias })}
                    />
                    <div className={CONFIG_PAIR}>
                        <InheritableList
                            label="只启用这些功能"
                            value={rule.enable}
                            fallback={defaults.enable}
                            disabled={disabled}
                            onChange={(enable) => set({ enable })}
                        />
                        <InheritableList
                            label="禁用功能"
                            value={rule.disable}
                            fallback={defaults.disable}
                            disabled={disabled}
                            onChange={(disable) => set({ disable })}
                        />
                    </div>
                </div>
            </ExpandPresence>
        </div>
    );
};

/** 列表项：关着「单独设置」就沿用默认（写回时不写这个键） */
const InheritableList: React.FC<{
    label: string;
    value: string[] | undefined;
    fallback: string[];
    disabled?: boolean;
    onChange: (next: string[] | undefined) => void;
}> = ({ label, value, fallback, disabled, onChange }) => {
    const own = value != null;
    return (
        <div className="flex flex-col gap-2">
            <Switch
                label={`${label}单独设置`}
                checked={own}
                disabled={disabled}
                onCheckedChange={(on) => onChange(on ? [...fallback] : undefined)}
            />
            {own ? (
                <StringListField value={value} mono={false} disabled={disabled} onChange={onChange} />
            ) : (
                <p className="text-2xs text-text-tertiary">
                    沿用默认：{fallback.length ? fallback.join('、') : '空'}
                </p>
            )}
        </div>
    );
};
