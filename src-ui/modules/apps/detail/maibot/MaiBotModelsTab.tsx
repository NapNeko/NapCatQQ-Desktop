// 模型：model_config 的全部内容。提供商 → 模型 → 任务是按名字串起来的，改名时引用跟着走，
// 删掉被引用的会在引用处标红，不替用户悄悄删引用。

import { AlertTriangle, Plus } from 'lucide-react';
import {
    Button,
    FormSection,
    Popover,
    PopoverClose,
    PopoverContent,
    PopoverTrigger,
    Switch,
} from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { ConfigForm } from '../karin/configLayout';
import {
    MAIBOT_PROVIDER_PRESETS,
    MAIBOT_REQUIRED_TASKS,
    MAIBOT_TASK_KEYS,
    maibotModelSetupIssue,
    newMaiBotProvider,
    renameMaiBotModel,
    renameMaiBotProvider,
    type MaiBotModelSetupIssue,
    type MaiBotProviderPreset,
} from '../../../../core/domain/apps/maibotConfig';
import { fieldOf, MODEL_SCHEMA, newItemFor, nodeAt } from '../../../../core/domain/apps/maibotSchema';
import type {
    MaiBotAPIProvider,
    MaiBotInstanceConfig,
    MaiBotModelConfigFile,
    MaiBotModelInfo,
} from '../../../../core/ipc/types';
import { ModelCard, ProviderCard, TaskCard } from './maibotModelCards';
import { ModelIdPicker, ProviderProbe } from './maibotProbes';

type TaskKey = (typeof MAIBOT_TASK_KEYS)[number];

const TASKS: Readonly<Record<TaskKey, { title: string; hint: string }>> = {
    replyer: { title: '回复', hint: '影响麦麦说话的表现' },
    planner: { title: '规划', hint: '决定麦麦做什么，要有一定 Agent 能力' },
    utils: { title: '杂务', hint: '概括、整理这类小活，挑个快的小模型' },
    vlm: { title: '识图', hint: '要能看图的模型' },
    voice: { title: '语音识别', hint: '把语音转成文字' },
    embedding: { title: '嵌入', hint: '文本嵌入模型，不能用对话模型；长期记忆靠它' },
    memory: { title: '长期记忆', hint: '总结、抽取、写回记忆；留空时按需回退' },
    mid_memory: { title: '聊天回想', hint: '留空用规划的模型' },
    learner: { title: '学习', hint: '学表达方式和黑话；留空用杂务的模型' },
    expression_use: { title: '用表达方式', hint: '留空用杂务的模型' },
    emoji: { title: '表情包', hint: '留空按规划 / 识图的模型选' },
};

// 必需的三个排前面，其余照上游顺序
const TASK_ORDER: readonly TaskKey[] = [
    ...MAIBOT_REQUIRED_TASKS,
    ...MAIBOT_TASK_KEYS.filter((k) => !(MAIBOT_REQUIRED_TASKS as readonly string[]).includes(k)),
];

const TASK_NODE = nodeAt(MODEL_SCHEMA, ['model_task_config']);
const MODEL_NODE = nodeAt(MODEL_SCHEMA, ['models']);

const SETUP_TEXT: Record<Exclude<MaiBotModelSetupIssue, null>, string> = {
    no_provider: '还没有提供商。先加一个，填上接口地址和 API Key。',
    placeholder_key: '默认带的提供商还是占位的 API Key，换成你自己的 Key 麦麦才能说话。',
    no_task_model: '回复、规划、杂务三个任务都要挑模型，不然麦麦说不了话。',
};

const hostOf = (url: string) => url.replace(/^https?:\/\//, '').replace(/\/.*$/, '');

/** 预设十几条，不限高会顶出窗口；和 AstrBot 模型页的预设菜单同一个样子 */
const PresetMenu: React.FC<{ onPick: (p: MaiBotProviderPreset) => void; disabled?: boolean }> = ({
    onPick,
    disabled,
}) => (
    <Popover>
        <PopoverTrigger asChild>
            <Button size="sm" variant="secondary" disabled={disabled}>
                <ActionMotionIcon icon={Plus} size={13} />
                加提供商
            </Button>
        </PopoverTrigger>
        <PopoverContent align="end" sideOffset={6} className="w-72 p-1">
            <div
                className="overflow-y-auto overscroll-contain"
                style={{
                    maxHeight: 'min(24rem, calc(var(--radix-popover-content-available-height, 24rem) - 8px))',
                }}
            >
                {MAIBOT_PROVIDER_PRESETS.map((p) => (
                    <PopoverClose key={p.id} asChild>
                        <button
                            type="button"
                            className="flex w-full items-center justify-between gap-3 rounded-sm px-2 py-1.5 text-left text-[13px] text-text hover:bg-inset"
                            onClick={() => onPick(p)}
                        >
                            <span className="shrink-0 whitespace-nowrap">{p.label}</span>
                            {p.base_url && (
                                <span className="min-w-0 truncate font-mono text-2xs text-text-tertiary">
                                    {hostOf(p.base_url)}
                                </span>
                            )}
                        </button>
                    </PopoverClose>
                ))}
            </div>
        </PopoverContent>
    </Popover>
);

function freshModel(models: MaiBotModelConfigFile): MaiBotModelInfo {
    const base = (MODEL_NODE ? newItemFor(MODEL_NODE) : {}) as MaiBotModelInfo;
    // extra_params 在 IPC 上是行内 TOML 文本，schema 里它是字典
    return { ...base, extra_params: '{}', api_provider: models.api_providers[0]?.name ?? '' };
}

export const MaiBotModelsTab: React.FC<{
    config: MaiBotInstanceConfig;
    onChange: (next: MaiBotInstanceConfig) => void;
    errors: Record<string, string>;
    disabled?: boolean;
    showAdvanced: boolean;
    onShowAdvanced: (next: boolean) => void;
    instanceId: string;
    /** 麦麦在跑且 WebUI 应答了：测连接、拉模型列表要走它 */
    live: boolean;
}> = ({ config, onChange, errors, disabled, showAdvanced, onShowAdvanced, instanceId, live }) => {
    const models = config.models;
    const setModels = (next: MaiBotModelConfigFile) => onChange({ ...config, models: next });
    const issue = maibotModelSetupIssue(models);
    const providerNames = models.api_providers.map((p) => p.name).filter(Boolean);

    const setProvider = (i: number, p: MaiBotAPIProvider) =>
        setModels({ ...models, api_providers: models.api_providers.map((x, j) => (j === i ? p : x)) });
    const setModel = (i: number, m: MaiBotModelInfo) =>
        setModels({ ...models, models: models.models.map((x, j) => (j === i ? m : x)) });

    // 从服务商列表加一条：名字默认就是标识，撞了往后编号
    const addModelFrom = (provider: string, identifier: string) => {
        const taken = new Set(models.models.map((m) => m.name));
        let name = identifier;
        for (let n = 2; taken.has(name); n += 1) name = `${identifier}-${n}`;
        const next = { ...freshModel(models), api_provider: provider, model_identifier: identifier, name };
        setModels({ ...models, models: [...models.models, next] });
    };

    // 换标识时，名字原本就跟着标识走的（或空着）一起换，任务里的引用跟过去
    const pickIdentifier = (i: number, identifier: string) => {
        const m = models.models[i];
        if (!m) return;
        const follow = !m.name || m.name === m.model_identifier;
        const next = { ...m, model_identifier: identifier, name: follow ? identifier : m.name };
        const updated = { ...models, models: models.models.map((x, j) => (j === i ? next : x)) };
        setModels(follow && m.name ? renameMaiBotModel(updated, m.name, identifier) : updated);
    };

    const tasks = TASK_ORDER.filter(
        (k) => showAdvanced || !fieldOf(TASK_NODE, k)?.advanced || models.model_task_config[k].model_list.length > 0,
    );

    return (
        <ConfigForm>
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                    {issue && (
                        <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/5 px-3 py-2.5 text-[13px] leading-relaxed text-text">
                            <AlertTriangle size={15} className="mt-0.5 shrink-0 text-warning" />
                            <span>{SETUP_TEXT[issue]}</span>
                        </div>
                    )}
                </div>
                <Switch label="显示高级选项" checked={showAdvanced} onCheckedChange={onShowAdvanced} />
            </div>

            <FormSection
                title="提供商"
                description="模型服务的接口地址和 Key"
                actions={
                    <PresetMenu
                        disabled={disabled}
                        onPick={(preset) =>
                            setModels({
                                ...models,
                                api_providers: [...models.api_providers, newMaiBotProvider(models.api_providers, preset)],
                            })
                        }
                    />
                }
            >
                {errors['models/api_providers'] && <p className="text-2xs text-danger">{errors['models/api_providers']}</p>}
                {models.api_providers.map((p, i) => (
                    <ProviderCard
                        key={i}
                        provider={p}
                        path={`models/api_providers/${i}`}
                        errors={errors}
                        disabled={disabled}
                        showAdvanced={showAdvanced}
                        onChange={(next) => setProvider(i, next)}
                        onRename={(from, to) => setModels(renameMaiBotProvider(models, from, to))}
                        onRemove={() =>
                            setModels({ ...models, api_providers: models.api_providers.filter((_, j) => j !== i) })
                        }
                        footer={
                            live && (
                                <ProviderProbe
                                    instanceId={instanceId}
                                    provider={p}
                                    added={
                                        new Set(
                                            models.models
                                                .filter((m) => m.api_provider === p.name)
                                                .map((m) => m.model_identifier),
                                        )
                                    }
                                    onAddModel={(id) => addModelFrom(p.name, id)}
                                    disabled={disabled}
                                />
                            )
                        }
                    />
                ))}
            </FormSection>

            <FormSection
                title="模型"
                description="每个模型挂在一个提供商下，名字给下面的任务挑"
                actions={
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={disabled}
                        onClick={() => setModels({ ...models, models: [...models.models, freshModel(models)] })}
                    >
                        <ActionMotionIcon icon={Plus} size={13} />
                        加模型
                    </Button>
                }
            >
                {errors['models/models'] && <p className="text-2xs text-danger">{errors['models/models']}</p>}
                {models.models.map((m, i) => (
                    <ModelCard
                        key={i}
                        model={m}
                        path={`models/models/${i}`}
                        errors={errors}
                        providers={providerNames}
                        disabled={disabled}
                        showAdvanced={showAdvanced}
                        onChange={(next) => setModel(i, next)}
                        onRename={(from, to) => setModels(renameMaiBotModel(models, from, to))}
                        onRemove={() => setModels({ ...models, models: models.models.filter((_, j) => j !== i) })}
                        identifierExtra={
                            live && (
                                <ModelIdPicker
                                    instanceId={instanceId}
                                    provider={models.api_providers.find((p) => p.name === m.api_provider)}
                                    current={m.model_identifier}
                                    onPick={(id) => pickIdentifier(i, id)}
                                    disabled={disabled}
                                />
                            )
                        }
                    />
                ))}
            </FormSection>

            <FormSection title="任务分配" description="每个任务从上面的模型里挑，可以挑多个轮换">
                {tasks.map((k) => (
                    <TaskCard
                        key={k}
                        title={TASKS[k].title}
                        hint={TASKS[k].hint}
                        required={(MAIBOT_REQUIRED_TASKS as readonly string[]).includes(k)}
                        task={models.model_task_config[k]}
                        path={`models/model_task_config/${k}`}
                        errors={errors}
                        models={models.models}
                        disabled={disabled}
                        showAdvanced={showAdvanced}
                        onChange={(next) =>
                            setModels({ ...models, model_task_config: { ...models.model_task_config, [k]: next } })
                        }
                    />
                ))}
            </FormSection>
        </ConfigForm>
    );
};
