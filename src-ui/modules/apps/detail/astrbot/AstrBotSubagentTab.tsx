// 子代理编排：每个子代理是「模型 + 人格」一对，列表一行一个，点进对话框选。
// 两样东西都在别的页配，缺哪个就把路指出来。

import { useState } from 'react';
import { Bot, Plus, Route, Trash2 } from 'lucide-react';
import { Badge, Button, FormSection, Select, Switch, SyntaxTextEditor, TextField } from '../../../../shared/ui';
import { ConfigForm } from '../karin/configLayout';
import { EmptyHint, EntityRow, FormDialog, JumpLink, Pill } from './parts';
import { enabledChatModels } from '../../../../core/domain/apps/astrbotConfig';
import { useAstrBotSubagentTools } from '../../../../hooks/apps/useAstrBotDashboard';
import { dashboardReady } from './AstrBotRuntimeGate';
import type {
    AstrBotDashboardStatus,
    AstrBotInstanceConfig,
    AstrBotPersona,
    AstrBotSubagentRow,
} from '../../../../core/ipc/types';

type AgentDraft = { index: number | null; agent: AstrBotSubagentRow };
type RouterDraft = Pick<AstrBotInstanceConfig['subagent'], 'router_system_prompt' | 'remove_main_duplicate_tools'>;

export const AstrBotSubagentTab: React.FC<{
    instanceId: string;
    config: AstrBotInstanceConfig;
    onChange: (next: AstrBotInstanceConfig) => void;
    disabled?: boolean;
    status: AstrBotDashboardStatus | undefined;
    personas: AstrBotPersona[];
    onGoTab: (tab: string) => void;
}> = ({ instanceId, config, onChange, disabled, status, personas, onGoTab }) => {
    const live = dashboardReady(status);
    const tools = useAstrBotSubagentTools(instanceId, live);
    const models = enabledChatModels(config);
    const modelItems = models.map((m) => ({ value: m.id, label: m.model || m.id }));
    const personaItems = personas.map((p) => ({ value: p.persona_id, label: p.persona_id }));
    const sub = config.subagent;
    const agents = sub.agents;
    const set = (patch: Partial<AstrBotInstanceConfig['subagent']>) => onChange({ ...config, subagent: { ...sub, ...patch } });

    const [agentDraft, setAgentDraft] = useState<AgentDraft | null>(null);
    const [routerDraft, setRouterDraft] = useState<RouterDraft | null>(null);

    const noModels = models.length === 0;
    const incomplete = agents.filter((a) => !a.provider_id || !a.persona_id).length;
    const modelLabel = (id: string) => modelItems.find((m) => m.value === id)?.label ?? id;

    const commitAgent = (d: AgentDraft) => {
        const agent = { provider_id: d.agent.provider_id, persona_id: d.agent.persona_id.trim() };
        set({ agents: d.index === null ? [...agents, agent] : agents.map((a, i) => (i === d.index ? agent : a)) });
        setAgentDraft(null);
    };

    const addButton = (
        <Button
            size="sm"
            variant="secondary"
            disabled={disabled || noModels}
            title={noModels ? '先要有对话模型' : undefined}
            onClick={() => setAgentDraft({ index: null, agent: { provider_id: modelItems[0]?.value ?? '', persona_id: '' } })}
        >
            <Plus size={13} /> 添加子代理
        </Button>
    );

    return (
        <ConfigForm>
            <FormSection
                title="子代理"
                actions={
                    <>
                        <Switch
                            label="启用"
                            checked={sub.main_enable}
                            disabled={disabled}
                            hint={
                                sub.main_enable && agents.length === 0
                                    ? '还没有子代理，开了也派不出去'
                                    : sub.main_enable && incomplete > 0
                                      ? `${incomplete} 个子代理没配全`
                                      : undefined
                            }
                            onCheckedChange={(main_enable) => set({ main_enable })}
                        />
                        {agents.length > 0 && addButton}
                    </>
                }
                layout="none"
            >
                {agents.length === 0 ? (
                    <EmptyHint
                        icon={Bot}
                        title={
                            noModels ? (
                                <>
                                    子代理要挂在对话模型上，现在一个都没有。
                                    <JumpLink tab="models" onGo={onGoTab} />
                                </>
                            ) : (
                                '还没有子代理。每个子代理是一对「模型 + 人格」，主模型按需要转交给它们'
                            )
                        }
                        action={noModels ? undefined : addButton}
                    />
                ) : (
                    <div className="flex flex-col gap-2">
                        {agents.map((a, i) => (
                            <EntityRow
                                key={i}
                                icon={Bot}
                                title={<span className="font-mono">{a.provider_id ? modelLabel(a.provider_id) : '没选模型'}</span>}
                                tags={!a.provider_id || !a.persona_id ? <Badge tone="warning">没配全</Badge> : undefined}
                                subtitle={a.persona_id ? `人格 ${a.persona_id}` : '没选人格'}
                                actions={
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        className="h-7 w-7 text-danger hover:text-danger"
                                        aria-label="删除子代理"
                                        disabled={disabled}
                                        onClick={() => set({ agents: agents.filter((_, idx) => idx !== i) })}
                                    >
                                        <Trash2 size={14} />
                                    </Button>
                                }
                                onOpen={() => setAgentDraft({ index: i, agent: a })}
                            />
                        ))}
                    </div>
                )}
            </FormSection>

            <FormSection title="路由" layout="none">
                <EntityRow
                    icon={Route}
                    title="路由提示词"
                    tags={sub.remove_main_duplicate_tools ? <Badge tone="neutral">主会话去重工具</Badge> : undefined}
                    subtitle={sub.router_system_prompt || '没写，主模型自己判断什么时候转交'}
                    onOpen={() =>
                        setRouterDraft({
                            router_system_prompt: sub.router_system_prompt,
                            remove_main_duplicate_tools: sub.remove_main_duplicate_tools,
                        })
                    }
                />
            </FormSection>

            {(tools.data ?? []).length > 0 && (
                <FormSection title="可用工具" layout="none">
                    <div className="flex flex-wrap gap-1.5">
                        {tools.data?.map((name) => <Pill key={name}>{name}</Pill>)}
                    </div>
                </FormSection>
            )}

            {agentDraft && (
                <FormDialog
                    open
                    title={agentDraft.index === null ? '添加子代理' : '编辑子代理'}
                    confirmLabel={agentDraft.index === null ? '添加' : '确定'}
                    confirmDisabled={!agentDraft.agent.provider_id}
                    onCancel={() => setAgentDraft(null)}
                    onConfirm={() => commitAgent(agentDraft)}
                >
                    <AgentFields
                        agent={agentDraft.agent}
                        modelItems={modelItems}
                        personaItems={personaItems}
                        live={live}
                        onGoTab={(tab) => {
                            setAgentDraft(null);
                            onGoTab(tab);
                        }}
                        onChange={(agent) => setAgentDraft({ ...agentDraft, agent })}
                    />
                </FormDialog>
            )}

            {routerDraft && (
                <FormDialog
                    open
                    size="lg"
                    title="路由"
                    onCancel={() => setRouterDraft(null)}
                    onConfirm={() => {
                        set(routerDraft);
                        setRouterDraft(null);
                    }}
                >
                    <div className="flex flex-col gap-1.5">
                        <label className="text-xs font-medium text-text-secondary">路由提示词</label>
                        <div className="flex h-40 flex-col">
                            <SyntaxTextEditor
                                mode="plain"
                                wrap
                                value={routerDraft.router_system_prompt}
                                aria-label="路由提示词"
                                onChange={(router_system_prompt) => setRouterDraft({ ...routerDraft, router_system_prompt })}
                            />
                        </div>
                    </div>
                    <Switch
                        label="主会话不再重复挂载子代理的工具"
                        checked={routerDraft.remove_main_duplicate_tools}
                        onCheckedChange={(remove_main_duplicate_tools) => setRouterDraft({ ...routerDraft, remove_main_duplicate_tools })}
                    />
                </FormDialog>
            )}
        </ConfigForm>
    );
};

const AgentFields: React.FC<{
    agent: AstrBotSubagentRow;
    modelItems: { value: string; label: string }[];
    personaItems: { value: string; label: string }[];
    live: boolean;
    onGoTab: (tab: string) => void;
    onChange: (a: AstrBotSubagentRow) => void;
}> = ({ agent, modelItems, personaItems, live, onGoTab, onChange }) => {
    const modelKnown = modelItems.some((m) => m.value === agent.provider_id);
    const personaKnown = personaItems.some((p) => p.value === agent.persona_id);
    const noPersonas = personaItems.length === 0;
    return (
        <>
            <Select
                label="模型"
                value={agent.provider_id || undefined}
                items={modelKnown || !agent.provider_id ? modelItems : [{ value: agent.provider_id, label: agent.provider_id }, ...modelItems]}
                placeholder="选择模型"
                onValueChange={(provider_id) => onChange({ ...agent, provider_id })}
            />
            {live ? (
                <Select
                    label="人格"
                    value={agent.persona_id || undefined}
                    items={personaKnown || !agent.persona_id ? personaItems : [{ value: agent.persona_id, label: agent.persona_id }, ...personaItems]}
                    placeholder={noPersonas ? '还没有人格' : '选择人格'}
                    disabled={noPersonas && !agent.persona_id}
                    hint={noPersonas ? <JumpLink tab="persona" onGo={onGoTab}>去「人格」页新建</JumpLink> : undefined}
                    onValueChange={(persona_id) => onChange({ ...agent, persona_id })}
                />
            ) : (
                <TextField
                    label="人格"
                    value={agent.persona_id}
                    className="font-mono"
                    hint="实例运行时可以直接选，现在只能填标识"
                    onValueChange={(persona_id) => onChange({ ...agent, persona_id })}
                />
            )}
        </>
    );
};
