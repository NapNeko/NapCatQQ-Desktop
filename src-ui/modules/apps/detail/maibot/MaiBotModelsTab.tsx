// 模型：model_config 的全部内容。提供商 → 模型 → 任务是按名字串起来的，改名时引用跟着走，
// 删掉被引用的会在引用处标红，不替用户悄悄删引用。

import { AlertTriangle, Plus } from 'lucide-react';
import { Button, FormSection, Switch } from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';
import { ConfigForm } from '../karin/configLayout';
import {
    MAIBOT_REQUIRED_TASKS,
    MAIBOT_TASK_KEYS,
    maibotModelSetupIssue,
    renameMaiBotModel,
    renameMaiBotProvider,
    type MaiBotModelSetupIssue,
} from '../../../../core/domain/apps/maibotConfig';
import { fieldOf, MODEL_SCHEMA, newItemFor, nodeAt } from '../../../../core/domain/apps/maibotSchema';
import type {
    MaiBotAPIProvider,
    MaiBotInstanceConfig,
    MaiBotModelConfigFile,
    MaiBotModelInfo,
} from '../../../../core/ipc/types';
import { ModelCard, ProviderCard, TaskCard } from './maibotModelCards';

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
const PROVIDER_NODE = nodeAt(MODEL_SCHEMA, ['api_providers']);
const MODEL_NODE = nodeAt(MODEL_SCHEMA, ['models']);

const SETUP_TEXT: Record<Exclude<MaiBotModelSetupIssue, null>, string> = {
    no_provider: '还没有提供商。先加一个，填上接口地址和 API Key。',
    placeholder_key: '默认带的提供商还是占位的 API Key，换成你自己的 Key 麦麦才能说话。',
    no_task_model: '回复、规划、杂务三个任务都要挑模型，不然麦麦说不了话。',
};

function freshProvider(existing: readonly MaiBotAPIProvider[]): MaiBotAPIProvider {
    const base = (PROVIDER_NODE ? newItemFor(PROVIDER_NODE) : {}) as MaiBotAPIProvider;
    let n = existing.length + 1;
    while (existing.some((p) => p.name === `提供商 ${n}`)) n += 1;
    return { ...base, name: `提供商 ${n}` };
}

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
}> = ({ config, onChange, errors, disabled, showAdvanced, onShowAdvanced }) => {
    const models = config.models;
    const setModels = (next: MaiBotModelConfigFile) => onChange({ ...config, models: next });
    const issue = maibotModelSetupIssue(models);
    const providerNames = models.api_providers.map((p) => p.name).filter(Boolean);

    const setProvider = (i: number, p: MaiBotAPIProvider) =>
        setModels({ ...models, api_providers: models.api_providers.map((x, j) => (j === i ? p : x)) });
    const setModel = (i: number, m: MaiBotModelInfo) =>
        setModels({ ...models, models: models.models.map((x, j) => (j === i ? m : x)) });

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
                    <Button
                        size="sm"
                        variant="secondary"
                        disabled={disabled}
                        onClick={() =>
                            setModels({ ...models, api_providers: [...models.api_providers, freshProvider(models.api_providers)] })
                        }
                    >
                        <ActionMotionIcon icon={Plus} size={13} />
                        加提供商
                    </Button>
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
