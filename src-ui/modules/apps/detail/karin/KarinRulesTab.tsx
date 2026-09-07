// Karin「响应规则」：groups / privates。匹配顺序收进 Popover，不常驻。

import { Plus } from 'lucide-react';
import { Button, FormSection, Popover, PopoverContent, PopoverTrigger } from '../../../../shared/ui';
import { KARIN_GROUP_RULE_KEYS, KARIN_PRIVATE_RULE_KEYS, newGroupRule, newPrivateRule } from '../../../../core/domain/apps/karinConfig';
import { ConfigForm } from './configLayout';
import { ScopedRuleEditor } from './ScopedRuleEditor';
import type { KarinTabProps } from './KarinBasicTab';

export const KarinRulesTab: React.FC<KarinTabProps> = ({ config, onChange, errors, disabled }) => {
    return (
        <ConfigForm>
            <FormSection
                title="群 / 频道"
                actions={
                    <div className="flex items-center gap-2">
                        <MatchOrderPopover
                            text="Bot:selfId:guildId:channelId → Bot:selfId:groupId → Bot:selfId → global → default。开启继承时未设置的项沿用上级。"
                        />
                        <Button
                            size="sm"
                            variant="secondary"
                            disabled={disabled}
                            onClick={() => onChange({ ...config, groups: [...config.groups, newGroupRule()] })}
                        >
                            <Plus size={13} /> 添加
                        </Button>
                    </div>
                }
                layout="none"
            >
                {config.groups.map((rule, i) => (
                    <ScopedRuleEditor
                        key={`${rule.key}-${i}`}
                        rule={rule}
                        index={i}
                        kind="group"
                        keyTemplates={KARIN_GROUP_RULE_KEYS}
                        errors={errors}
                        disabled={disabled}
                        onChange={(next) =>
                            onChange({ ...config, groups: config.groups.map((r, j) => (j === i ? next : r)) })
                        }
                        onRemove={() => onChange({ ...config, groups: config.groups.filter((_, j) => j !== i) })}
                    />
                ))}
            </FormSection>

            <FormSection
                title="私聊"
                actions={
                    <div className="flex items-center gap-2">
                        <MatchOrderPopover text="Bot:selfId:userId → Bot:selfId → global → default。" />
                        <Button
                            size="sm"
                            variant="secondary"
                            disabled={disabled}
                            onClick={() => onChange({ ...config, privates: [...config.privates, newPrivateRule()] })}
                        >
                            <Plus size={13} /> 添加
                        </Button>
                    </div>
                }
                layout="none"
            >
                {config.privates.map((rule, i) => (
                    <ScopedRuleEditor
                        key={`${rule.key}-${i}`}
                        rule={rule}
                        index={i}
                        kind="private"
                        keyTemplates={KARIN_PRIVATE_RULE_KEYS}
                        errors={errors}
                        disabled={disabled}
                        onChange={(next) =>
                            onChange({ ...config, privates: config.privates.map((r, j) => (j === i ? next : r)) })
                        }
                        onRemove={() => onChange({ ...config, privates: config.privates.filter((_, j) => j !== i) })}
                    />
                ))}
            </FormSection>
        </ConfigForm>
    );
};

const MatchOrderPopover: React.FC<{ text: string }> = ({ text }) => (
    <Popover>
        <PopoverTrigger asChild>
            <button
                type="button"
                className="inline-flex h-6 items-center rounded-sm px-1.5 text-[11px] text-text-tertiary transition-colors hover:bg-inset hover:text-text"
            >
                匹配顺序
            </button>
        </PopoverTrigger>
        <PopoverContent side="bottom" align="end" sideOffset={6}>
            <p className="max-w-xs font-mono text-[12px] leading-relaxed text-text-secondary">{text}</p>
        </PopoverContent>
    </Popover>
);
