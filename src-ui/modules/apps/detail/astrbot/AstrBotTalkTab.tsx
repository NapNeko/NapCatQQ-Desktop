// 回复设置：什么时候回、回谁、怎么回。四块直接铺开，不再藏进对话框。
// 默认模型在模型页、默认人格在人格页、挂哪些知识库在知识库页，这里不重复放。

import {
    FormSection,
    NumberField,
    Select,
    StringListField,
    Switch,
    TextField,
} from '../../../../shared/ui';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import { modelsOfType } from '../../../../core/domain/apps/astrbotConfig';
import { JumpLink } from './parts';
import type { AstrBotAiSettings, AstrBotInstanceConfig } from '../../../../core/ipc/types';

const SWITCH_ROW = 'flex flex-wrap items-center gap-x-8 gap-y-3';

const STRATEGY_LABEL: Record<string, string> = {
    truncate_by_turns: '丢掉最早的几轮',
    llm_compress: '让模型压缩',
};

export const AstrBotTalkTab: React.FC<{
    config: AstrBotInstanceConfig;
    onChange: (next: AstrBotInstanceConfig) => void;
    disabled?: boolean;
    onGoTab: (tab: string) => void;
}> = ({ config, onChange, disabled, onGoTab }) => {
    const a = config.ai;
    const g = config.gates;
    const setAi = (patch: Partial<AstrBotAiSettings>) =>
        onChange({ ...config, ai: { ...a, ...patch } });
    const setGates = (patch: Partial<AstrBotInstanceConfig['gates']>) =>
        onChange({ ...config, gates: { ...g, ...patch } });
    const limitOn = a.max_context_length >= 0;
    const stt = modelsOfType(config, 'speech_to_text');
    const tts = modelsOfType(config, 'text_to_speech');

    return (
        <ConfigForm>
            <FormSection
                title="大模型回复"
                description={a.enable ? undefined : '关着的时候只响应指令和插件'}
                actions={
                    <Switch
                        label="启用"
                        checked={a.enable}
                        disabled={disabled}
                        onCheckedChange={(enable) => setAi({ enable })}
                    />
                }
            >
                <TextField
                    label="提示词模板"
                    value={a.prompt_prefix}
                    disabled={disabled}
                    className="font-mono"
                    hint="{{prompt}} 是用户说的话；不写占位符就加在用户输入前面"
                    onValueChange={(prompt_prefix) => setAi({ prompt_prefix })}
                />
                <div className={SWITCH_ROW}>
                    <Switch
                        label="流式回复"
                        checked={a.streaming_response}
                        disabled={disabled}
                        onCheckedChange={(streaming_response) => setAi({ streaming_response })}
                    />
                    <Switch
                        label="安全模式"
                        checked={a.llm_safety_mode}
                        disabled={disabled}
                        onCheckedChange={(llm_safety_mode) => setAi({ llm_safety_mode })}
                    />
                </div>
            </FormSection>

            <FormSection title="触发与权限">
                <div className={CONFIG_PAIR}>
                    <StringListField
                        label="唤醒前缀"
                        value={g.wake_prefix}
                        disabled={disabled}
                        onChange={(wake_prefix) => setGates({ wake_prefix })}
                    />
                    <TextField
                        label="对话额外前缀"
                        value={a.wake_prefix}
                        disabled={disabled}
                        hint="唤醒之后还要带这个前缀才走大模型；留空就不额外要求"
                        onValueChange={(wake_prefix) => setAi({ wake_prefix })}
                    />
                </div>
                <div className={SWITCH_ROW}>
                    <Switch
                        label="私聊也要唤醒前缀"
                        checked={g.friend_message_needs_wake_prefix}
                        disabled={disabled}
                        onCheckedChange={(friend_message_needs_wake_prefix) =>
                            setGates({ friend_message_needs_wake_prefix })
                        }
                    />
                    <Switch
                        label="独立会话"
                        checked={g.unique_session}
                        disabled={disabled}
                        hint="群成员各自一段上下文"
                        onCheckedChange={(unique_session) => setGates({ unique_session })}
                    />
                    <Switch
                        label="只回白名单里的会话"
                        checked={g.enable_id_white_list}
                        disabled={disabled}
                        // 上游 whitelist_check：名单为空直接跳过检查，所以开着也是谁都回
                        hint={
                            g.enable_id_white_list && g.id_whitelist.length === 0
                                ? '名单为空时不限制，谁都回'
                                : undefined
                        }
                        onCheckedChange={(enable_id_white_list) =>
                            setGates({ enable_id_white_list })
                        }
                    />
                </div>
                <div className={CONFIG_PAIR}>
                    <StringListField
                        label="白名单"
                        value={g.id_whitelist}
                        mono
                        disabled={disabled}
                        hint="填群号或会话 ID"
                        onChange={(id_whitelist) => setGates({ id_whitelist })}
                    />
                    <StringListField
                        label="管理员"
                        value={g.admins_id}
                        mono
                        disabled={disabled}
                        onChange={(admins_id) => setGates({ admins_id })}
                    />
                </div>
            </FormSection>

            <FormSection title="上下文与工具">
                <Switch
                    label="限制上下文长度"
                    checked={limitOn}
                    disabled={disabled}
                    onCheckedChange={(on) => setAi({ max_context_length: on ? 20 : -1 })}
                />
                <div className={CONFIG_PAIR}>
                    {limitOn && (
                        <>
                            <NumberField
                                label="保留轮数"
                                value={a.max_context_length}
                                min={0}
                                disabled={disabled}
                                onValueChange={(max_context_length) =>
                                    setAi({
                                        max_context_length:
                                            max_context_length ?? a.max_context_length,
                                    })
                                }
                            />
                            <Select
                                label="超长时"
                                value={a.context_limit_reached_strategy}
                                items={Object.entries(STRATEGY_LABEL).map(([value, label]) => ({
                                    value,
                                    label,
                                }))}
                                disabled={disabled}
                                onValueChange={(context_limit_reached_strategy) =>
                                    setAi({ context_limit_reached_strategy })
                                }
                            />
                        </>
                    )}
                    <NumberField
                        label="工具调用步数上限"
                        value={a.max_agent_step}
                        min={1}
                        disabled={disabled}
                        onValueChange={(max_agent_step) =>
                            setAi({ max_agent_step: max_agent_step ?? a.max_agent_step })
                        }
                    />
                    <NumberField
                        label="工具调用超时（秒）"
                        value={a.tool_call_timeout}
                        min={1}
                        disabled={disabled}
                        onValueChange={(tool_call_timeout) =>
                            setAi({ tool_call_timeout: tool_call_timeout ?? a.tool_call_timeout })
                        }
                    />
                </div>
            </FormSection>

            <FormSection title="语音与搜索">
                <div className={CONFIG_PAIR}>
                    <ProviderToggle
                        label="语音转文字"
                        enabled={config.stt.enable}
                        providerId={config.stt.provider_id}
                        options={stt.map((m) => ({ value: m.id, label: m.model || m.id }))}
                        disabled={disabled}
                        onGoTab={onGoTab}
                        onEnable={(enable) =>
                            onChange({ ...config, stt: { ...config.stt, enable } })
                        }
                        onProvider={(provider_id) =>
                            onChange({ ...config, stt: { ...config.stt, provider_id } })
                        }
                    />
                    <ProviderToggle
                        label="文字转语音"
                        enabled={config.tts.enable}
                        providerId={config.tts.provider_id}
                        options={tts.map((m) => ({ value: m.id, label: m.model || m.id }))}
                        disabled={disabled}
                        onGoTab={onGoTab}
                        onEnable={(enable) =>
                            onChange({ ...config, tts: { ...config.tts, enable } })
                        }
                        onProvider={(provider_id) =>
                            onChange({ ...config, tts: { ...config.tts, provider_id } })
                        }
                    />
                </div>
                <Switch
                    label="联网搜索"
                    checked={config.websearch.enable}
                    disabled={disabled}
                    onCheckedChange={(enable) =>
                        onChange({ ...config, websearch: { ...config.websearch, enable } })
                    }
                />
            </FormSection>
        </ConfigForm>
    );
};

/** 开关 + 提供商下拉绑一起：开了却没选提供商等于没开，所以选不到就把路指出来。 */
const ProviderToggle: React.FC<{
    label: string;
    enabled: boolean;
    providerId: string;
    options: { value: string; label: string }[];
    disabled?: boolean;
    onGoTab: (tab: string) => void;
    onEnable: (v: boolean) => void;
    onProvider: (id: string) => void;
}> = ({ label, enabled, providerId, options, disabled, onGoTab, onEnable, onProvider }) => {
    const known = options.some((o) => o.value === providerId);
    const items =
        known || !providerId ? options : [{ value: providerId, label: providerId }, ...options];
    const none = items.length === 0;
    return (
        <div className="flex flex-col gap-2">
            <Switch
                label={label}
                checked={enabled}
                disabled={disabled}
                onCheckedChange={onEnable}
            />
            {enabled && (
                <Select
                    label="提供商"
                    value={providerId || undefined}
                    items={items}
                    placeholder={none ? '还没有这类提供商，开关不会生效' : '选择提供商'}
                    disabled={disabled || none}
                    error={!none && !providerId ? '没选提供商，开关不会生效' : undefined}
                    hint={
                        none ? (
                            <JumpLink tab="models" onGo={onGoTab}>
                                去「模型」页加一个「{label}」提供商
                            </JumpLink>
                        ) : undefined
                    }
                    onValueChange={onProvider}
                />
            )}
        </div>
    );
};
