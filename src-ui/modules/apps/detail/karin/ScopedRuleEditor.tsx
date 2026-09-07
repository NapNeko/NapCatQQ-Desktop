// 群 / 私聊规则卡片。default / global 是兜底，不能删、不能改键。

import { useState } from 'react';
import { Trash2 } from 'lucide-react';
import { Badge, Button, NumberField, Select, StringListField, Switch, TextField } from '../../../../shared/ui';
import { ExpandChevron, ExpandPresence } from '../../../../shared/ui/motion';
import { KARIN_RULE_MODES } from '../../../../core/domain/apps/karinConfig';
import { CONFIG_PAIR } from './configLayout';
import type { KarinScopeRule } from '../../../../core/ipc/types';

const MODE_ITEMS = KARIN_RULE_MODES.map((m) => ({ value: String(m.value), label: m.label }));
const RESERVED_KEYS = new Set(['default', 'global']);

export interface ScopedRuleEditorProps {
    rule: KarinScopeRule;
    index: number;
    kind: 'group' | 'private';
    keyTemplates: ReadonlyArray<string>;
    errors: Record<string, string>;
    disabled?: boolean;
    onChange: (next: KarinScopeRule) => void;
    onRemove: () => void;
}

export const ScopedRuleEditor: React.FC<ScopedRuleEditorProps> = ({
    rule,
    index,
    kind,
    keyTemplates,
    errors,
    disabled,
    onChange,
    onRemove,
}) => {
    const root = kind === 'group' ? 'groups' : 'privates';
    const reserved = RESERVED_KEYS.has(rule.key);
    const [open, setOpen] = useState(false);
    const set = (patch: Partial<KarinScopeRule>) => onChange({ ...rule, ...patch });
    const modeMeta = KARIN_RULE_MODES.find((m) => m.value === rule.mode);
    const templateValue = (keyTemplates as readonly string[]).includes(rule.key) ? rule.key : '__custom__';
    const keyError = errors[`${root}/${index}/key`];

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
                    <span className="truncate font-mono text-sm text-text">{rule.key || '（未命名）'}</span>
                    {reserved && (
                        <Badge tone="neutral" appearance="outline">
                            兜底
                        </Badge>
                    )}
                    <span className="truncate text-2xs text-text-tertiary">{modeMeta?.label ?? `mode ${rule.mode}`}</span>
                    {keyError && <span className="text-2xs text-danger">{keyError}</span>}
                </button>
                <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-danger hover:text-danger"
                    aria-label="删除规则"
                    disabled={disabled || reserved}
                    title={reserved ? 'default / global 不能删' : undefined}
                    onClick={onRemove}
                >
                    <Trash2 size={14} />
                </Button>
            </div>

            <ExpandPresence visible={open}>
                <div className="flex flex-col gap-5 pt-4">
                    <div className={CONFIG_PAIR}>
                        <Select
                            label="键模板"
                            items={[
                                ...keyTemplates.map((k) => ({ value: k, label: k })),
                                { value: '__custom__', label: '自定义' },
                            ]}
                            value={templateValue}
                            disabled={disabled || reserved}
                            onValueChange={(v) => {
                                if (v !== '__custom__') set({ key: v });
                            }}
                        />
                        <TextField
                            label="规则键"
                            error={keyError}
                            value={rule.key}
                            disabled={disabled || reserved}
                            className="font-mono"
                            onValueChange={(key) => set({ key })}
                        />
                    </div>
                    <Select
                        label="响应模式"
                        error={errors[`${root}/${index}/mode`]}
                        items={MODE_ITEMS}
                        value={String(rule.mode)}
                        disabled={disabled}
                        onValueChange={(v) => set({ mode: Number(v) })}
                    />
                    {kind === 'group' ? (
                        <div className={CONFIG_PAIR}>
                            <NumberField
                                label="群冷却（秒）"
                                value={rule.cd}
                                min={0}
                                disabled={disabled}
                                onValueChange={(v) => set({ cd: v ?? 0 })}
                            />
                            <NumberField
                                label="单人冷却（秒）"
                                value={rule.userCD ?? 0}
                                min={0}
                                disabled={disabled}
                                onValueChange={(v) => set({ userCD: v ?? 0 })}
                            />
                        </div>
                    ) : (
                        <NumberField
                            label="冷却（秒）"
                            value={rule.cd}
                            min={0}
                            disabled={disabled}
                            onValueChange={(v) => set({ cd: v ?? 0 })}
                        />
                    )}
                    <Switch
                        label="继承上级"
                        checked={rule.inherit}
                        disabled={disabled}
                        onCheckedChange={(inherit) => set({ inherit })}
                    />
                    <StringListField
                        label="别名"
                        value={rule.alias}
                        disabled={disabled}
                        mono={false}
                        onChange={(alias) => set({ alias })}
                    />
                    <div className={CONFIG_PAIR}>
                        <StringListField
                            label="启用插件"
                            value={rule.enable}
                            disabled={disabled}
                            onChange={(enable) => set({ enable })}
                        />
                        <StringListField
                            label="禁用插件"
                            value={rule.disable}
                            disabled={disabled}
                            onChange={(disable) => set({ disable })}
                        />
                        {kind === 'group' && (
                            <>
                                <StringListField
                                    label="成员白名单"
                                    value={rule.member_enable ?? []}
                                    disabled={disabled}
                                    onChange={(member_enable) => set({ member_enable })}
                                />
                                <StringListField
                                    label="成员黑名单"
                                    value={rule.member_disable ?? []}
                                    disabled={disabled}
                                    onChange={(member_disable) => set({ member_disable })}
                                />
                            </>
                        )}
                    </div>
                </div>
            </ExpandPresence>
        </div>
    );
};
