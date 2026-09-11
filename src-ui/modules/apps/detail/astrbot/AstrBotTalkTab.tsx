// 对话行为。页面上只留启用 / 默认模型 / 默认人格 / 备用模型；触发权限、知识库检索、上下文工具、
// 语音搜索各自一行摘要，点进对话框改。凡是依赖别的页配出来的东西（模型、人格、知识库），选不到就给跳转。

import { useState } from 'react';
import { AudioLines, Brain, Library, ShieldCheck } from 'lucide-react';
import type { LucideProps } from 'lucide-react';
import type { ComponentType } from 'react';
import {
    Badge,
    FormSection,
    NumberField,
    Select,
    StringListField,
    Switch,
    TextField,
} from '../../../../shared/ui';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import { enabledChatModels, modelsOfType } from '../../../../core/domain/apps/astrbotConfig';
import { EntityRow, FormDialog, JumpLink, PickList } from './parts';
import type {
    AstrBotAiSettings,
    AstrBotInstanceConfig,
    AstrBotKnowledgeBase,
    AstrBotPersona,
} from '../../../../core/ipc/types';

const SWITCH_ROW = 'flex flex-wrap items-center gap-x-8 gap-y-3';

type SectionKey = 'gates' | 'kb' | 'context' | 'speech';

const STRATEGY_LABEL: Record<string, string> = {
    truncate_by_turns: '丢掉最早的几轮',
    llm_compress: '让模型压缩',
};

type Summary = { text: string; warn?: string };

function gatesSummary(cfg: AstrBotInstanceConfig): Summary {
    const g = cfg.gates;
    const parts = [g.wake_prefix.length ? `唤醒前缀 ${g.wake_prefix.join(' ')}` : '不用唤醒前缀'];
    if (g.unique_session) parts.push('独立会话');
    if (g.enable_id_white_list) parts.push(g.id_whitelist.length ? `白名单 ${g.id_whitelist.length} 个` : '白名单为空');
    parts.push(`管理员 ${g.admins_id.length}`);
    return {
        text: parts.join(' · '),
        warn: g.enable_id_white_list && g.id_whitelist.length === 0 ? '谁都不回' : undefined,
    };
}

function kbSummary(cfg: AstrBotInstanceConfig): Summary {
    const k = cfg.kb;
    const parts = [k.names.length ? `挂载 ${k.names.join('、')}` : '未挂载知识库', `召回 ${k.fusion_top_k} → ${k.final_top_k}`];
    if (k.agentic_mode) parts.push('模型自主检索');
    return { text: parts.join(' · ') };
}

function contextSummary(cfg: AstrBotInstanceConfig): Summary {
    const a = cfg.ai;
    const parts = [
        a.max_context_length >= 0
            ? `保留 ${a.max_context_length} 轮 · ${STRATEGY_LABEL[a.context_limit_reached_strategy] ?? a.context_limit_reached_strategy}`
            : '不限上下文',
    ];
    if (a.streaming_response) parts.push('流式');
    if (a.llm_safety_mode) parts.push('安全模式');
    parts.push(`工具 ${a.max_agent_step} 步 / ${a.tool_call_timeout}s`);
    return { text: parts.join(' · ') };
}

function speechSummary(cfg: AstrBotInstanceConfig): Summary {
    const on: string[] = [];
    let warn: string | undefined;
    if (cfg.stt.enable) {
        on.push('语音转文字');
        if (!cfg.stt.provider_id) warn = '语音转文字没选提供商';
    }
    if (cfg.tts.enable) {
        on.push('文字转语音');
        if (!cfg.tts.provider_id) warn = warn ? '两项都没选提供商' : '文字转语音没选提供商';
    }
    if (cfg.websearch.enable) on.push('联网搜索');
    return { text: on.length ? on.join(' · ') : '全部关闭', warn };
}

const SECTIONS: { key: SectionKey; title: string; icon: ComponentType<LucideProps>; summary: (c: AstrBotInstanceConfig) => Summary }[] = [
    { key: 'gates', title: '触发与权限', icon: ShieldCheck, summary: gatesSummary },
    { key: 'kb', title: '知识库检索', icon: Library, summary: kbSummary },
    { key: 'context', title: '上下文与工具', icon: Brain, summary: contextSummary },
    { key: 'speech', title: '语音与搜索', icon: AudioLines, summary: speechSummary },
];

export const AstrBotTalkTab: React.FC<{
    config: AstrBotInstanceConfig;
    onChange: (next: AstrBotInstanceConfig) => void;
    disabled?: boolean;
    personas: AstrBotPersona[];
    kbs: AstrBotKnowledgeBase[];
    /** Dashboard 通了才有人格 / 知识库列表；没通就退回手填 */
    live: boolean;
    onGoTab: (tab: string) => void;
}> = ({ config, onChange, disabled, personas, kbs, live, onGoTab }) => {
    const [section, setSection] = useState<SectionKey | null>(null);
    const [local, setLocal] = useState<AstrBotInstanceConfig | null>(null);

    const chat = enabledChatModels(config);
    const modelItems = chat.map((m) => ({ value: m.id, label: m.model || m.id }));
    const personaItems = personas.map((p) => ({ value: p.persona_id, label: p.persona_id }));
    const setAi = (patch: Partial<AstrBotAiSettings>) => onChange({ ...config, ai: { ...config.ai, ...patch } });

    const currentPersona = config.ai.default_personality;
    const personaKnown = personaItems.some((i) => i.value === currentPersona);

    const openSection = (key: SectionKey) => {
        setLocal(config);
        setSection(key);
    };
    const close = () => {
        setSection(null);
        setLocal(null);
    };
    const confirm = () => {
        if (local) onChange(local);
        close();
    };
    // 从对话框里跳去别的页：先把改的写回，别让人回来发现白改
    const jump = (tab: string) => {
        confirm();
        onGoTab(tab);
    };

    const current = SECTIONS.find((s) => s.key === section);

    return (
        <ConfigForm>
            <FormSection
                title="对话"
                actions={
                    <Switch
                        label="启用"
                        checked={config.ai.enable}
                        disabled={disabled}
                        onCheckedChange={(enable) => setAi({ enable })}
                    />
                }
            >
                <div className={CONFIG_PAIR}>
                    <Select
                        label="默认模型"
                        value={modelItems.some((i) => i.value === config.ai.default_provider_id) ? config.ai.default_provider_id : undefined}
                        items={modelItems}
                        placeholder={chat.length ? '选择模型' : '还没有可用的对话模型'}
                        disabled={disabled || chat.length === 0}
                        hint={chat.length === 0 ? <JumpLink tab="models" onGo={onGoTab}>去「模型」页加一个</JumpLink> : undefined}
                        onValueChange={(default_provider_id) => setAi({ default_provider_id })}
                    />
                    <Select
                        label="默认人格"
                        value={currentPersona || undefined}
                        items={personaKnown || !currentPersona ? personaItems : [{ value: currentPersona, label: currentPersona }, ...personaItems]}
                        placeholder={personaItems.length ? '选择人格' : live ? '还没有人格' : '实例运行时才能列出'}
                        disabled={disabled || (personaItems.length === 0 && !currentPersona)}
                        hint={
                            live ? (
                                <JumpLink tab="persona" onGo={onGoTab}>{personaItems.length ? '管理人格' : '去「人格」页新建'}</JumpLink>
                            ) : undefined
                        }
                        onValueChange={(default_personality) => setAi({ default_personality })}
                    />
                </div>
                <PickList
                    label="备用模型"
                    options={modelItems.filter((m) => m.value !== config.ai.default_provider_id)}
                    value={config.ai.fallback_chat_models}
                    disabled={disabled}
                    empty={chat.length <= 1 ? '至少要有两个对话模型才谈得上备用' : '没有可选的备用模型'}
                    hint={config.ai.fallback_chat_models.length ? '默认模型出错时按顺序换用' : undefined}
                    onChange={(fallback_chat_models) => setAi({ fallback_chat_models })}
                />
            </FormSection>

            <FormSection title="更多设置" layout="none">
                <div className="flex flex-col gap-2">
                    {SECTIONS.map((s) => {
                        const sum = s.summary(config);
                        return (
                            <EntityRow
                                key={s.key}
                                icon={s.icon}
                                title={s.title}
                                tags={sum.warn ? <Badge tone="warning">{sum.warn}</Badge> : undefined}
                                subtitle={sum.text}
                                onOpen={() => openSection(s.key)}
                            />
                        );
                    })}
                </div>
            </FormSection>

            {current && local && (
                <FormDialog open size="lg" title={current.title} onCancel={close} onConfirm={confirm}>
                    {section === 'gates' && <GatesFields cfg={local} set={setLocal} />}
                    {section === 'kb' && <KbFields cfg={local} set={setLocal} kbs={kbs} live={live} onGoTab={jump} />}
                    {section === 'context' && <ContextFields cfg={local} set={setLocal} />}
                    {section === 'speech' && <SpeechFields cfg={local} set={setLocal} onGoTab={jump} />}
                </FormDialog>
            )}
        </ConfigForm>
    );
};

type FieldsProps = { cfg: AstrBotInstanceConfig; set: (next: AstrBotInstanceConfig) => void };

const GatesFields: React.FC<FieldsProps> = ({ cfg, set }) => {
    const g = cfg.gates;
    const setGates = (patch: Partial<AstrBotInstanceConfig['gates']>) => set({ ...cfg, gates: { ...g, ...patch } });
    return (
        <>
            <div className={CONFIG_PAIR}>
                <StringListField label="唤醒前缀" value={g.wake_prefix} onChange={(wake_prefix) => setGates({ wake_prefix })} />
                <TextField
                    label="对话额外前缀"
                    value={cfg.ai.wake_prefix}
                    hint="唤醒之后还要带这个前缀才走大模型；留空就不额外要求"
                    onValueChange={(wake_prefix) => set({ ...cfg, ai: { ...cfg.ai, wake_prefix } })}
                />
            </div>
            <div className={SWITCH_ROW}>
                <Switch label="独立会话" checked={g.unique_session} onCheckedChange={(unique_session) => setGates({ unique_session })} />
                <Switch
                    label="私聊也要唤醒前缀"
                    checked={g.friend_message_needs_wake_prefix}
                    onCheckedChange={(friend_message_needs_wake_prefix) => setGates({ friend_message_needs_wake_prefix })}
                />
                <Switch
                    label="只回白名单"
                    checked={g.enable_id_white_list}
                    hint={g.enable_id_white_list && g.id_whitelist.length === 0 ? '名单是空的，现在谁都不回' : undefined}
                    onCheckedChange={(enable_id_white_list) => setGates({ enable_id_white_list })}
                />
            </div>
            <div className={CONFIG_PAIR}>
                <StringListField label="白名单" value={g.id_whitelist} mono onChange={(id_whitelist) => setGates({ id_whitelist })} />
                <StringListField label="管理员" value={g.admins_id} mono onChange={(admins_id) => setGates({ admins_id })} />
            </div>
        </>
    );
};

const KbFields: React.FC<FieldsProps & { kbs: AstrBotKnowledgeBase[]; live: boolean; onGoTab: (tab: string) => void }> = ({
    cfg,
    set,
    kbs,
    live,
    onGoTab,
}) => {
    const k = cfg.kb;
    const setKb = (patch: Partial<AstrBotInstanceConfig['kb']>) => set({ ...cfg, kb: { ...k, ...patch } });
    return (
        <>
            {live ? (
                <PickList
                    label="挂载的知识库"
                    options={kbs.map((x) => ({ value: x.kb_name, label: x.kb_name }))}
                    value={k.names}
                    empty={
                        <>
                            还没有知识库。<JumpLink tab="kb" onGo={onGoTab} />
                        </>
                    }
                    onChange={(names) => setKb({ names })}
                />
            ) : (
                <StringListField
                    label="挂载的知识库"
                    value={k.names}
                    hint="实例运行时可以直接勾选，现在只能填名字"
                    onChange={(names) => setKb({ names })}
                />
            )}
            <div className={CONFIG_PAIR}>
                <NumberField
                    label="召回条数"
                    value={k.fusion_top_k}
                    min={1}
                    onValueChange={(fusion_top_k) => setKb({ fusion_top_k: fusion_top_k ?? k.fusion_top_k })}
                />
                <NumberField
                    label="最终保留"
                    value={k.final_top_k}
                    min={1}
                    onValueChange={(final_top_k) => setKb({ final_top_k: final_top_k ?? k.final_top_k })}
                />
            </div>
            <Switch label="让模型自己决定何时检索" checked={k.agentic_mode} onCheckedChange={(agentic_mode) => setKb({ agentic_mode })} />
        </>
    );
};

const ContextFields: React.FC<FieldsProps> = ({ cfg, set }) => {
    const a = cfg.ai;
    const setAi = (patch: Partial<AstrBotAiSettings>) => set({ ...cfg, ai: { ...a, ...patch } });
    const limitOn = a.max_context_length >= 0;
    return (
        <>
            <TextField
                label="提示词模板"
                value={a.prompt_prefix}
                className="font-mono"
                onValueChange={(prompt_prefix) => setAi({ prompt_prefix })}
            />
            <div className={SWITCH_ROW}>
                <Switch label="限制上下文长度" checked={limitOn} onCheckedChange={(on) => setAi({ max_context_length: on ? 20 : -1 })} />
                <Switch label="流式回复" checked={a.streaming_response} onCheckedChange={(streaming_response) => setAi({ streaming_response })} />
                <Switch label="安全模式" checked={a.llm_safety_mode} onCheckedChange={(llm_safety_mode) => setAi({ llm_safety_mode })} />
            </div>
            <div className={CONFIG_PAIR}>
                {limitOn && (
                    <>
                        <NumberField
                            label="保留轮数"
                            value={a.max_context_length}
                            min={0}
                            onValueChange={(max_context_length) => setAi({ max_context_length: max_context_length ?? a.max_context_length })}
                        />
                        <Select
                            label="超长时"
                            value={a.context_limit_reached_strategy}
                            items={Object.entries(STRATEGY_LABEL).map(([value, label]) => ({ value, label }))}
                            onValueChange={(context_limit_reached_strategy) => setAi({ context_limit_reached_strategy })}
                        />
                    </>
                )}
                <NumberField
                    label="工具调用步数上限"
                    value={a.max_agent_step}
                    min={1}
                    onValueChange={(max_agent_step) => setAi({ max_agent_step: max_agent_step ?? a.max_agent_step })}
                />
                <NumberField
                    label="工具调用超时（秒）"
                    value={a.tool_call_timeout}
                    min={1}
                    onValueChange={(tool_call_timeout) => setAi({ tool_call_timeout: tool_call_timeout ?? a.tool_call_timeout })}
                />
            </div>
        </>
    );
};

const SpeechFields: React.FC<FieldsProps & { onGoTab: (tab: string) => void }> = ({ cfg, set, onGoTab }) => {
    const stt = modelsOfType(cfg, 'speech_to_text');
    const tts = modelsOfType(cfg, 'text_to_speech');
    return (
        <>
            <div className={CONFIG_PAIR}>
                <ProviderToggle
                    label="语音转文字"
                    enabled={cfg.stt.enable}
                    providerId={cfg.stt.provider_id}
                    options={stt.map((m) => ({ value: m.id, label: m.model || m.id }))}
                    onGoTab={onGoTab}
                    onEnable={(enable) => set({ ...cfg, stt: { ...cfg.stt, enable } })}
                    onProvider={(provider_id) => set({ ...cfg, stt: { ...cfg.stt, provider_id } })}
                />
                <ProviderToggle
                    label="文字转语音"
                    enabled={cfg.tts.enable}
                    providerId={cfg.tts.provider_id}
                    options={tts.map((m) => ({ value: m.id, label: m.model || m.id }))}
                    onGoTab={onGoTab}
                    onEnable={(enable) => set({ ...cfg, tts: { ...cfg.tts, enable } })}
                    onProvider={(provider_id) => set({ ...cfg, tts: { ...cfg.tts, provider_id } })}
                />
            </div>
            <Switch
                label="联网搜索"
                checked={cfg.websearch.enable}
                onCheckedChange={(enable) => set({ ...cfg, websearch: { ...cfg.websearch, enable } })}
            />
        </>
    );
};

/** 开关 + 提供商下拉绑一起：开了却没选提供商等于没开，所以选不到就把路指出来。 */
const ProviderToggle: React.FC<{
    label: string;
    enabled: boolean;
    providerId: string;
    options: { value: string; label: string }[];
    onGoTab: (tab: string) => void;
    onEnable: (v: boolean) => void;
    onProvider: (id: string) => void;
}> = ({ label, enabled, providerId, options, onGoTab, onEnable, onProvider }) => {
    const known = options.some((o) => o.value === providerId);
    const items = known || !providerId ? options : [{ value: providerId, label: providerId }, ...options];
    const none = items.length === 0;
    return (
        <div className="flex flex-col gap-2">
            <Switch label={label} checked={enabled} onCheckedChange={onEnable} />
            {enabled && (
                <Select
                    label="提供商"
                    value={providerId || undefined}
                    items={items}
                    placeholder={none ? '还没有这类提供商，开关不会生效' : '选择提供商'}
                    disabled={none}
                    error={!none && !providerId ? '没选提供商，开关不会生效' : undefined}
                    hint={none ? <JumpLink tab="models" onGo={onGoTab}>去「模型」页加一个「{label}」提供商</JumpLink> : undefined}
                    onValueChange={onProvider}
                />
            )}
        </div>
    );
};
