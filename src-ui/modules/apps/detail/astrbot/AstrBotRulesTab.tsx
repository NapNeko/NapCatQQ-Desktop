// 会话规则按会话分组，六个上游认的键各自有结构化编辑；配置档案这里只建 / 删，内容和绑定去 WebUI。

import { useMemo, useState } from 'react';
import { ChevronRight, ExternalLink, FileSliders, MessagesSquare, Plus, SlidersHorizontal, Trash2, Users } from 'lucide-react';
import { Badge, Button, Card, FormSection, Select, Switch, SyntaxTextEditor, TextField } from '../../../../shared/ui';
import { ConfigForm } from '../karin/configLayout';
import { AstrBotRuntimeGate, dashboardReady } from './AstrBotRuntimeGate';
import { ConfirmDelete, EmptyHint, EntityRow, FormDialog } from './parts';
import { useAstrBotAbconfs, useAstrBotSessionRules } from '../../../../hooks/apps/useAstrBotDashboard';
import {
    ASTRBOT_SESSION_RULE_KEYS,
    enabledChatModels,
    isKnownSessionRuleKey,
    modelsOfType,
    parseJsonString,
    parseSessionServiceConfig,
    type AstrBotSessionRuleKey,
} from '../../../../core/domain/apps/astrbotConfig';
import type {
    AstrBotAbconfInfo,
    AstrBotDashboardStatus,
    AstrBotInstanceConfig,
    AstrBotSessionRule,
} from '../../../../core/ipc/types';

const RULE_LABEL: Record<AstrBotSessionRuleKey, string> = {
    session_service_config: '会话开关',
    provider_perf_chat_completion: '对话模型',
    provider_perf_speech_to_text: '语音转文字提供商',
    provider_perf_text_to_speech: '文字转语音提供商',
    kb_config: '知识库（JSON）',
    session_plugin_config: '插件开关（JSON）',
};

const DEFAULT_JSON: Record<AstrBotSessionRuleKey, string> = {
    session_service_config: JSON.stringify(
        { session_enabled: true, llm_enabled: true, tts_enabled: true, custom_name: '' },
        null,
        2,
    ),
    provider_perf_chat_completion: '""',
    provider_perf_speech_to_text: '""',
    provider_perf_text_to_speech: '""',
    kb_config: '{}',
    session_plugin_config: '{}',
};

function badJson(text: string): boolean {
    try {
        JSON.parse(text);
        return false;
    } catch {
        return true;
    }
}

/** `aiocqhttp:GroupMessage:123456` → 平台 / 类型 / 会话号 */
function parseUmo(umo: string): { platform: string; kind: 'group' | 'friend' | 'other'; id: string } {
    const [platform = '', type = '', ...rest] = umo.split(':');
    const t = type.toLowerCase();
    const kind = t.includes('group') ? 'group' : t.includes('friend') || t.includes('private') ? 'friend' : 'other';
    return { platform, kind, id: rest.join(':') };
}

type Draft = { mode: 'create' | 'edit'; rule: AstrBotSessionRule };

export const AstrBotRulesTab: React.FC<{
    instanceId: string;
    status: AstrBotDashboardStatus | undefined;
    statusLoading: boolean;
    config: AstrBotInstanceConfig;
    onOpenWebUi: (path: string) => void;
    onGoTab: (tab: string) => void;
    onStart: () => void;
    starting: boolean;
}> = ({ instanceId, status, statusLoading, config, onOpenWebUi, onGoTab, onStart, starting }) => {
    const ready = dashboardReady(status);
    const ab = useAstrBotAbconfs(instanceId, ready);
    const rules = useAstrBotSessionRules(instanceId, ready);
    const [draft, setDraft] = useState<Draft | null>(null);
    const [name, setName] = useState<string | null>(null);
    const [pendingRule, setPendingRule] = useState<AstrBotSessionRule | null>(null);
    const [pendingAbconf, setPendingAbconf] = useState<AstrBotAbconfInfo | null>(null);

    const list = rules.data ?? [];
    const sessions = useMemo(() => {
        const map = new Map<string, AstrBotSessionRule[]>();
        for (const r of list) {
            const arr = map.get(r.umo) ?? [];
            arr.push(r);
            map.set(r.umo, arr);
        }
        return [...map.entries()];
    }, [list]);

    const modelName = (id: string) => config.models.find((m) => m.id === id)?.model || id;

    const summarize = (r: AstrBotSessionRule): string => {
        if (!isKnownSessionRuleKey(r.rule_key)) return r.rule_json;
        switch (r.rule_key) {
            case 'session_service_config': {
                const c = parseSessionServiceConfig(r.rule_json);
                const parts = [
                    c.custom_name && `「${c.custom_name}」`,
                    `会话${c.session_enabled ? '开' : '关'}`,
                    `大模型${c.llm_enabled ? '开' : '关'}`,
                    `语音${c.tts_enabled ? '开' : '关'}`,
                ].filter(Boolean);
                return parts.join(' · ');
            }
            case 'provider_perf_chat_completion':
            case 'provider_perf_speech_to_text':
            case 'provider_perf_text_to_speech': {
                const id = parseJsonString(r.rule_json);
                return id ? modelName(id) : '没选';
            }
            default:
                return r.rule_json;
        }
    };

    const openCreate = (umo = '') =>
        setDraft({
            mode: 'create',
            rule: { umo, rule_key: 'session_service_config', rule_json: DEFAULT_JSON.session_service_config },
        });

    const addButton = (
        <Button size="sm" variant="secondary" disabled={!ready} onClick={() => openCreate()}>
            <Plus size={13} /> 添加规则
        </Button>
    );

    const abList = ab.data ?? [];

    return (
        <ConfigForm>
            <AstrBotRuntimeGate
                status={status}
                loading={statusLoading}
                onGoTab={onGoTab}
                onStart={onStart}
                starting={starting}
            />

            <FormSection
                title="会话规则"
                description="按会话覆盖通用配置，比如某个群单独换模型、某个人关掉大模型"
                actions={sessions.length > 0 ? addButton : undefined}
                layout="none"
            >
                {sessions.length === 0 ? (
                    <EmptyHint
                        icon={SlidersHorizontal}
                        title={ready ? '还没有会话规则，所有会话都用通用配置' : '连上控制台后可以管理会话规则'}
                        action={ready ? addButton : undefined}
                    />
                ) : (
                    <div className="flex flex-col gap-3">
                        {sessions.map(([umo, items]) => {
                            const u = parseUmo(umo);
                            const Icon = u.kind === 'group' ? Users : MessagesSquare;
                            const usedKeys = new Set(items.map((r) => r.rule_key));
                            const canAddMore = ASTRBOT_SESSION_RULE_KEYS.some((k) => !usedKeys.has(k));
                            return (
                                <Card key={umo} variant="outlined" padding="none">
                                    <div className="flex items-center gap-3 px-4 py-2.5">
                                        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-inset text-text-tertiary">
                                            <Icon size={15} />
                                        </span>
                                        <div className="flex min-w-0 flex-1 items-center gap-2">
                                            <p className="truncate font-mono text-[13px] font-medium text-text" title={umo}>
                                                {u.id || umo}
                                            </p>
                                            {u.kind !== 'other' && (
                                                <Badge tone="neutral">{u.kind === 'group' ? '群' : '私聊'}</Badge>
                                            )}
                                            {u.platform && <Badge tone="info" className="font-mono">{u.platform}</Badge>}
                                        </div>
                                        <Button
                                            size="sm"
                                            variant="ghost"
                                            disabled={!ready || !canAddMore}
                                            onClick={() => {
                                                const key = ASTRBOT_SESSION_RULE_KEYS.find((k) => !usedKeys.has(k)) ?? 'session_service_config';
                                                setDraft({
                                                    mode: 'create',
                                                    rule: { umo, rule_key: key, rule_json: DEFAULT_JSON[key] },
                                                });
                                            }}
                                        >
                                            <Plus size={12} /> 加规则
                                        </Button>
                                    </div>
                                    <div className="flex flex-col divide-y divide-border-subtle/70 border-t border-border-subtle">
                                        {items.map((r) => (
                                            <div key={r.rule_key} className="group flex items-center gap-3 px-4 py-2 transition-colors hover:bg-inset/30">
                                                <button
                                                    type="button"
                                                    className="min-w-0 flex-1 rounded-sm text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40 disabled:cursor-default"
                                                    disabled={!ready}
                                                    onClick={() => setDraft({ mode: 'edit', rule: r })}
                                                >
                                                    <span className="block text-[13px] text-text">
                                                        {isKnownSessionRuleKey(r.rule_key) ? RULE_LABEL[r.rule_key] : (
                                                            <span className="font-mono">{r.rule_key}</span>
                                                        )}
                                                    </span>
                                                    <span className="block truncate font-mono text-xs text-text-tertiary" title={r.rule_json}>
                                                        {summarize(r)}
                                                    </span>
                                                </button>
                                                <div className="flex shrink-0 items-center gap-0.5">
                                                    <Button
                                                        variant="ghost"
                                                        size="icon"
                                                        className="h-7 w-7 text-danger hover:text-danger"
                                                        aria-label="删除规则"
                                                        disabled={!ready}
                                                        onClick={() => setPendingRule(r)}
                                                    >
                                                        <Trash2 size={13} />
                                                    </Button>
                                                    <ChevronRight size={14} className="ml-1 text-text-disabled transition-colors group-hover:text-text-secondary" />
                                                </div>
                                            </div>
                                        ))}
                                    </div>
                                </Card>
                            );
                        })}
                    </div>
                )}
            </FormSection>

            <FormSection
                title="配置档案"
                description="桌面端只编辑默认档案。其它档案的内容、以及哪个会话用哪份档案，在 WebUI 里改"
                actions={
                    <>
                        <Button size="sm" variant="ghost" disabled={!ready} onClick={() => onOpenWebUi('/config')}>
                            <ExternalLink size={12} /> WebUI
                        </Button>
                        <Button size="sm" variant="ghost" disabled={!ready} onClick={() => setName('')}>
                            <Plus size={12} /> 新建
                        </Button>
                    </>
                }
                layout="none"
            >
                {abList.length === 0 ? (
                    <p className="text-xs text-text-tertiary">
                        {ready ? '只有默认档案' : '连上控制台后能看到档案列表'}
                    </p>
                ) : (
                    <div className="flex flex-col gap-2">
                        {abList.map((a) => (
                            <EntityRow
                                key={a.id}
                                icon={FileSliders}
                                title={a.name || a.id}
                                tags={
                                    a.id === 'default' ? (
                                        <Badge tone="brand">桌面端在编辑</Badge>
                                    ) : (
                                        <Badge tone="neutral" className="font-mono">{a.id}</Badge>
                                    )
                                }
                                actions={
                                    a.id !== 'default' ? (
                                        <Button
                                            variant="ghost"
                                            size="icon"
                                            className="h-7 w-7 text-danger hover:text-danger"
                                            aria-label="删除档案"
                                            disabled={!ready}
                                            onClick={() => setPendingAbconf(a)}
                                        >
                                            <Trash2 size={13} />
                                        </Button>
                                    ) : undefined
                                }
                            />
                        ))}
                    </div>
                )}
            </FormSection>

            {name !== null && (
                <FormDialog
                    open
                    size="sm"
                    title="新建配置档案"
                    confirmLabel="创建"
                    confirmDisabled={!name.trim()}
                    busy={ab.create.isPending}
                    onCancel={() => setName(null)}
                    onConfirm={() => {
                        void ab.create.mutateAsync(name.trim()).then(() => setName(null));
                    }}
                >
                    <TextField
                        label="档案名"
                        autoFocus
                        value={name}
                        hint="内容从默认档案复制一份；之后在 WebUI 里改"
                        onValueChange={setName}
                    />
                </FormDialog>
            )}

            <RuleDialog
                draft={draft}
                config={config}
                existing={list}
                busy={rules.update.isPending}
                onClose={() => setDraft(null)}
                onChange={(rule) => draft && setDraft({ ...draft, rule })}
                onSave={() => {
                    if (!draft) return;
                    void rules.update
                        .mutateAsync({ ...draft.rule, umo: draft.rule.umo.trim() })
                        .then(() => setDraft(null));
                }}
            />

            <ConfirmDelete
                open={!!pendingRule}
                title={`删除这条「${pendingRule && isKnownSessionRuleKey(pendingRule.rule_key) ? RULE_LABEL[pendingRule.rule_key] : pendingRule?.rule_key}」规则？`}
                description={pendingRule ? `会话 ${pendingRule.umo} 会退回通用配置。` : undefined}
                busy={rules.remove.isPending}
                onCancel={() => setPendingRule(null)}
                onConfirm={() => {
                    if (!pendingRule) return;
                    void rules.remove
                        .mutateAsync({ umo: pendingRule.umo, ruleKey: pendingRule.rule_key })
                        .then(() => setPendingRule(null));
                }}
            />

            <ConfirmDelete
                open={!!pendingAbconf}
                title={`删除档案「${pendingAbconf?.name || pendingAbconf?.id}」？`}
                description="绑在这份档案上的会话会退回默认档案，不能撤销。"
                busy={ab.remove.isPending}
                onCancel={() => setPendingAbconf(null)}
                onConfirm={() => {
                    if (!pendingAbconf) return;
                    void ab.remove.mutateAsync(pendingAbconf.id).then(() => setPendingAbconf(null));
                }}
            />
        </ConfigForm>
    );
};

const RuleDialog: React.FC<{
    draft: Draft | null;
    config: AstrBotInstanceConfig;
    existing: AstrBotSessionRule[];
    busy: boolean;
    onClose: () => void;
    onChange: (rule: AstrBotSessionRule) => void;
    onSave: () => void;
}> = ({ draft, config, existing, busy, onClose, onChange, onSave }) => {
    const rule = draft?.rule;
    const creating = draft?.mode === 'create';
    const umos = useMemo(() => [...new Set(existing.map((r) => r.umo))], [existing]);
    const usedKeys = new Set(existing.filter((r) => r.umo === rule?.umo.trim()).map((r) => r.rule_key));
    const knownKey = !!rule && isKnownSessionRuleKey(rule.rule_key);
    const structured = knownKey && rule.rule_key !== 'kb_config' && rule.rule_key !== 'session_plugin_config';
    const invalid = !!rule && !structured && badJson(rule.rule_json);
    const duplicate = creating && !!rule && usedKeys.has(rule.rule_key);
    const canSave = !!rule && !!rule.umo.trim() && !!rule.rule_key && !invalid && !duplicate && !busy;

    const keyItems = ASTRBOT_SESSION_RULE_KEYS.map((k) => ({
        value: k as string,
        label: usedKeys.has(k) && creating ? `${RULE_LABEL[k]}（已有）` : RULE_LABEL[k],
        disabled: creating && usedKeys.has(k),
    }));

    if (!rule) return null;
    return (
        <FormDialog
            open
            title={creating ? '添加会话规则' : '编辑会话规则'}
            confirmLabel={creating ? '添加' : '保存'}
            confirmDisabled={!canSave}
            busy={busy}
            onCancel={onClose}
            onConfirm={onSave}
        >
            <TextField
                label="会话"
                autoFocus={creating && !rule.umo}
                value={rule.umo}
                disabled={!creating}
                className="font-mono"
                placeholder="aiocqhttp:GroupMessage:123456"
                list="astrbot-rule-umos"
                hint={creating ? '平台:消息类型:会话号；群是 GroupMessage，私聊是 FriendMessage' : undefined}
                onValueChange={(umo) => onChange({ ...rule, umo })}
            />
            <datalist id="astrbot-rule-umos">
                {umos.map((u) => <option key={u} value={u} />)}
            </datalist>
            <Select
                label="规则"
                value={knownKey ? rule.rule_key : undefined}
                items={keyItems}
                disabled={!creating}
                error={duplicate ? '这个会话已经有这条规则，去编辑那条' : undefined}
                onValueChange={(rule_key) =>
                    onChange({
                        ...rule,
                        rule_key,
                        rule_json: isKnownSessionRuleKey(rule_key) ? DEFAULT_JSON[rule_key] : '{}',
                    })
                }
            />
            <RuleValueEditor rule={rule} config={config} onChange={(rule_json) => onChange({ ...rule, rule_json })} />
        </FormDialog>
    );
};

const RuleValueEditor: React.FC<{
    rule: AstrBotSessionRule;
    config: AstrBotInstanceConfig;
    onChange: (json: string) => void;
}> = ({ rule, config, onChange }) => {
    if (rule.rule_key === 'session_service_config') {
        const c = parseSessionServiceConfig(rule.rule_json);
        const patch = (p: Partial<typeof c>) => onChange(JSON.stringify({ ...c, ...p }));
        return (
            <div className="flex flex-col gap-3 rounded-md border border-border-subtle bg-inset/40 p-3">
                <div className="flex flex-wrap items-center gap-x-8 gap-y-3">
                    <Switch label="会话" checked={c.session_enabled} onCheckedChange={(v) => patch({ session_enabled: v })} />
                    <Switch label="大模型" checked={c.llm_enabled} onCheckedChange={(v) => patch({ llm_enabled: v })} />
                    <Switch label="语音合成" checked={c.tts_enabled} onCheckedChange={(v) => patch({ tts_enabled: v })} />
                </div>
                <TextField
                    label="备注名"
                    value={c.custom_name}
                    placeholder="给这个会话起个好认的名字"
                    onValueChange={(custom_name) => patch({ custom_name })}
                />
            </div>
        );
    }

    if (
        rule.rule_key === 'provider_perf_chat_completion' ||
        rule.rule_key === 'provider_perf_speech_to_text' ||
        rule.rule_key === 'provider_perf_text_to_speech'
    ) {
        const pool =
            rule.rule_key === 'provider_perf_chat_completion'
                ? enabledChatModels(config)
                : modelsOfType(
                      config,
                      rule.rule_key === 'provider_perf_speech_to_text' ? 'speech_to_text' : 'text_to_speech',
                  );
        const current = parseJsonString(rule.rule_json);
        const items = pool.map((m) => ({ value: m.id, label: m.model || m.id }));
        if (current && !items.some((i) => i.value === current)) items.unshift({ value: current, label: current });
        return (
            <Select
                label="用哪个"
                value={current || undefined}
                items={items}
                placeholder={items.length ? '选择' : '当前配置里没有这类模型'}
                disabled={items.length === 0}
                hint={items.length === 0 ? '先在「模型」页加好并保存' : undefined}
                onValueChange={(id) => onChange(JSON.stringify(id))}
            />
        );
    }

    return (
        <div className="flex flex-col gap-1.5">
            <label className="text-xs font-medium text-text-secondary">规则内容（JSON）</label>
            <div className="flex h-36 flex-col">
                <SyntaxTextEditor
                    mode="json"
                    wrap
                    value={rule.rule_json}
                    invalid={badJson(rule.rule_json)}
                    aria-label="规则内容"
                    onChange={onChange}
                />
            </div>
        </div>
    );
};
