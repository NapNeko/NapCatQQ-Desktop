// 模型提供商列表。页面上只有一行一个提供商（id / 类型 / 地址 / 模型药丸），改什么都进对话框。
// 对话框里是局部草稿，确定才写回表单；拉取模型只对服务端已认（已保存）的源开放。

import { useState } from 'react';
import { Boxes, ChevronDown, Plus, RefreshCw, Star, Trash2 } from 'lucide-react';
import {
    Badge,
    Button,
    FormSection,
    NumberField,
    Popover,
    PopoverClose,
    PopoverContent,
    PopoverTrigger,
    Select,
    Switch,
    TextField,
} from '../../../../shared/ui';
import { ExpandChevron, ExpandPresence } from '../../../../shared/ui/motion';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import { EmptyHint, EntityRow, FormDialog, Pill } from './parts';
import {
    ASTRBOT_PROVIDER_TYPES,
    ASTRBOT_SOURCE_PRESETS,
    isChatProviderType,
    isKnownProviderType,
    newOpenAiModel,
    newSourceFromPreset,
    presetLabel,
    uniqueId,
    type AstrBotSourcePreset,
} from '../../../../core/domain/apps/astrbotConfig';
import { appFrameworkService } from '../../../../core/services/app-framework.service';
import { toAppConfigError } from '../../../../core/domain/apps/appConfigError';
import { pushAppErrorBar } from '../../../../hooks/apps/pushAppErrorBar';
import { cn } from '../../../../shared/utils/cn';
import type { AstrBotInstanceConfig, AstrBotProviderModel, AstrBotProviderSource } from '../../../../core/ipc/types';

const TYPE_LABEL: Record<string, string> = {
    chat_completion: '对话',
    speech_to_text: '语音转文字',
    text_to_speech: '文字转语音',
    embedding: '向量嵌入',
    rerank: '重排',
    agent_runner: '智能体',
};

const TYPE_ITEMS = ASTRBOT_PROVIDER_TYPES.map((v) => ({ value: v, label: TYPE_LABEL[v] ?? v }));

function hostOf(apiBase: string): string {
    try {
        return new URL(apiBase).host;
    } catch {
        return apiBase;
    }
}

/** 对话框里的局部草稿。index 为 null 表示新增；defaultId 是全局默认模型的副本，确定时一并写回。 */
type ProviderDraft = {
    index: number | null;
    original: AstrBotProviderSource | null;
    src: AstrBotProviderSource;
    kids: AstrBotProviderModel[];
    defaultId: string;
};

export const AstrBotModelsTab: React.FC<{
    config: AstrBotInstanceConfig;
    /** 最近一次保存成功的版本；没在里面的源服务端不认，拉不了模型 */
    saved: AstrBotInstanceConfig | null;
    onChange: (next: AstrBotInstanceConfig) => void;
    errors: Record<string, string>;
    disabled?: boolean;
    running: boolean;
    instanceId: string;
}> = ({ config, saved, onChange, errors, disabled, running, instanceId }) => {
    const [draft, setDraft] = useState<ProviderDraft | null>(null);
    const savedIds = new Set((saved?.sources ?? []).map((s) => s.id));

    const openEdit = (index: number) => {
        const src = config.sources[index];
        setDraft({
            index,
            original: src,
            src,
            kids: config.models.filter((m) => m.provider_source_id === src.id),
            defaultId: config.ai.default_provider_id,
        });
    };

    const openCreate = (preset: AstrBotSourcePreset) =>
        setDraft({
            index: null,
            original: null,
            src: newSourceFromPreset(preset, config.sources),
            kids: [],
            defaultId: config.ai.default_provider_id,
        });

    const commit = (d: ProviderDraft) => {
        const id = d.src.id.trim();
        const src = { ...d.src, id };
        const kids = d.kids.map((m) => ({ ...m, provider_source_id: id }));
        const sources = d.index === null ? [...config.sources, src] : config.sources.map((s, i) => (i === d.index ? src : s));
        // 原位替换这一源的模型，别把它们挪到列表尾巴上改变配置文件顺序
        const models: AstrBotProviderModel[] = [];
        let placed = false;
        for (const m of config.models) {
            if (d.original && m.provider_source_id === d.original.id) {
                if (!placed) {
                    models.push(...kids);
                    placed = true;
                }
                continue;
            }
            models.push(m);
        }
        if (!placed) models.push(...kids);
        const default_provider_id = models.some((m) => m.id === d.defaultId) ? d.defaultId : '';
        onChange({ ...config, sources, models, ai: { ...config.ai, default_provider_id } });
        setDraft(null);
    };

    const removeSource = (index: number) => {
        const id = config.sources[index]?.id;
        const rest = config.models.filter((m) => m.provider_source_id !== id);
        const defaultGone = !rest.some((m) => m.id === config.ai.default_provider_id);
        onChange({
            ...config,
            sources: config.sources.filter((_, i) => i !== index),
            models: rest,
            ai: defaultGone ? { ...config.ai, default_provider_id: '' } : config.ai,
        });
    };

    const toggleSource = (index: number, enable: boolean) =>
        onChange({ ...config, sources: config.sources.map((s, i) => (i === index ? { ...s, enable } : s)) });

    const rowHasError = (index: number, src: AstrBotProviderSource) =>
        Object.keys(errors).some((k) => {
            if (k.startsWith(`sources/${index}/`)) return true;
            const m = /^models\/(\d+)\//.exec(k);
            return m ? config.models[Number(m[1])]?.provider_source_id === src.id : false;
        });

    const addMenu = (
        <Popover>
            <PopoverTrigger asChild>
                <Button size="sm" variant="secondary" disabled={disabled}>
                    <Plus size={13} /> 添加提供商 <ChevronDown size={12} className="-mr-0.5 opacity-70" />
                </Button>
            </PopoverTrigger>
            <PopoverContent align="end" sideOffset={6} className="w-72 p-1">
                {/* 十几条预设不限高会顶出窗口；变量是触发器到窗口边的剩余空间，滚轮只在菜单里 */}
                <div
                    className="overflow-y-auto overscroll-contain"
                    style={{
                        maxHeight: 'min(24rem, calc(var(--radix-popover-content-available-height, 24rem) - 8px))',
                    }}
                >
                    <PresetGroup title="对话" items={ASTRBOT_SOURCE_PRESETS.filter((p) => p.provider_type === 'chat_completion')} onPick={openCreate} />
                    <div className="my-1 h-px bg-border-subtle" />
                    <PresetGroup title="向量嵌入（知识库用）" items={ASTRBOT_SOURCE_PRESETS.filter((p) => p.provider_type === 'embedding')} onPick={openCreate} />
                </div>
            </PopoverContent>
        </Popover>
    );

    return (
        <ConfigForm>
            <FormSection
                title="模型提供商"
                actions={config.sources.length > 0 ? addMenu : undefined}
                layout="none"
            >
                {config.sources.length === 0 ? (
                    <EmptyHint
                        icon={Boxes}
                        title="还没有模型提供商。选一家服务商，填上 Key 就能开始"
                        action={addMenu}
                    />
                ) : (
                    <div className="flex flex-col gap-2">
                        {config.sources.map((src, i) => {
                            const known = isKnownProviderType(src.provider_type);
                            const kids = config.models.filter((m) => m.provider_source_id === src.id);
                            const chat = isChatProviderType(src.provider_type);
                            return (
                                <EntityRow
                                    key={`${src.id}-${i}`}
                                    icon={Boxes}
                                    muted={!src.enable}
                                    title={<span className="font-mono">{src.id || '未命名'}</span>}
                                    tags={
                                        <>
                                            <Badge tone={known ? 'info' : 'neutral'}>
                                                {known ? TYPE_LABEL[src.provider_type] ?? src.provider_type : '桌面端只读'}
                                            </Badge>
                                            {!savedIds.has(src.id) && <Badge tone="warning">未保存</Badge>}
                                            {rowHasError(i, src) && <Badge tone="danger">有误</Badge>}
                                        </>
                                    }
                                    subtitle={[presetLabel(src), hostOf(src.api_base) || '接口地址未填'].filter(Boolean).join(' · ')}
                                    extra={
                                        kids.length ? (
                                            <span className="flex flex-wrap gap-1">
                                                {kids.map((m) => {
                                                    const isDefault = chat && m.id === config.ai.default_provider_id;
                                                    return (
                                                        <Pill
                                                            key={m.id}
                                                            className={cn(
                                                                'gap-1',
                                                                isDefault && 'bg-warning-soft text-warning',
                                                                !m.enable && 'line-through opacity-60',
                                                            )}
                                                        >
                                                            {isDefault && <Star size={9} fill="currentColor" />}
                                                            {m.model || m.id}
                                                        </Pill>
                                                    );
                                                })}
                                            </span>
                                        ) : (
                                            <span className="text-2xs text-text-tertiary">没有模型，点进去添加</span>
                                        )
                                    }
                                    actions={
                                        <>
                                            <Switch
                                                checked={src.enable}
                                                aria-label="启用提供商"
                                                disabled={disabled}
                                                onCheckedChange={(enable) => toggleSource(i, enable)}
                                            />
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                className="h-7 w-7 text-danger hover:text-danger"
                                                aria-label="删除提供商"
                                                title={kids.length ? `连同 ${kids.length} 个模型一起移出草稿，保存前可撤销` : undefined}
                                                disabled={disabled}
                                                onClick={() => removeSource(i)}
                                            >
                                                <Trash2 size={14} />
                                            </Button>
                                        </>
                                    }
                                    onOpen={() => openEdit(i)}
                                />
                            );
                        })}
                    </div>
                )}
            </FormSection>

            {draft && (
                <ProviderDialog
                    draft={draft}
                    config={config}
                    savedIds={savedIds}
                    running={running}
                    instanceId={instanceId}
                    errors={errors}
                    onChange={setDraft}
                    onCancel={() => setDraft(null)}
                    onConfirm={() => commit(draft)}
                />
            )}
        </ConfigForm>
    );
};

const ProviderDialog: React.FC<{
    draft: ProviderDraft;
    config: AstrBotInstanceConfig;
    savedIds: Set<string>;
    running: boolean;
    instanceId: string;
    errors: Record<string, string>;
    onChange: (d: ProviderDraft) => void;
    onCancel: () => void;
    onConfirm: () => void;
}> = ({ draft, config, savedIds, running, instanceId, errors, onChange, onCancel, onConfirm }) => {
    const [picks, setPicks] = useState<string[] | null>(null);
    const [fetching, setFetching] = useState(false);
    const [adv, setAdv] = useState(false);
    const [manual, setManual] = useState('');

    const { src, kids, original } = draft;
    const known = isKnownProviderType(src.provider_type);
    const chat = isChatProviderType(src.provider_type);
    const preset = ASTRBOT_SOURCE_PRESETS.find(
        (p) => p.type === src.type && p.provider === src.provider && p.provider_type === src.provider_type,
    );
    const isSaved = !!original && savedIds.has(original.id);

    const id = src.id.trim();
    const idError = !id
        ? '不能为空'
        : config.sources.some((s, i) => i !== draft.index && s.id === id)
          ? '已有同名提供商'
          : draft.index !== null
            ? errors[`sources/${draft.index}/id`]
            : undefined;
    const advOpen = adv || !!idError;

    const patchSrc = (patch: Partial<AstrBotProviderSource>) => onChange({ ...draft, src: { ...src, ...patch } });
    const patchModel = (mid: string, patch: Partial<AstrBotProviderModel>) =>
        onChange({ ...draft, kids: kids.map((m) => (m.id === mid ? { ...m, ...patch } : m)) });
    const removeModel = (mid: string) =>
        onChange({
            ...draft,
            kids: kids.filter((m) => m.id !== mid),
            defaultId: draft.defaultId === mid ? '' : draft.defaultId,
        });
    const addModel = (name: string) => {
        const trimmed = name.trim();
        if (!trimmed || kids.some((m) => m.model === trimmed)) return;
        const row = newOpenAiModel(id || src.id, trimmed);
        const taken = [
            ...config.models.filter((m) => !original || m.provider_source_id !== original.id).map((m) => m.id),
            ...kids.map((m) => m.id),
        ];
        const mid = uniqueId(row.id, taken);
        onChange({
            ...draft,
            kids: [...kids, { ...row, id: mid }],
            // 第一条对话模型直接当默认，省一次点击；已经有默认的不动
            defaultId: !draft.defaultId && chat ? mid : draft.defaultId,
        });
    };

    const canFetch = running && isSaved && !fetching;
    const fetchTitle = !running ? '实例跑起来后才能拉取' : !isSaved ? '先保存，服务端才认这个提供商' : undefined;
    const fetchModels = async () => {
        if (!original) return;
        setFetching(true);
        try {
            setPicks(await appFrameworkService.astrbotListSourceModels(instanceId, original.id));
        } catch (e) {
            pushAppErrorBar({
                key: `astrbot-models:${instanceId}:${original.id}`,
                title: '拉取模型列表失败',
                raw: toAppConfigError(e).message,
            });
        } finally {
            setFetching(false);
        }
    };
    const unpicked = (picks ?? []).filter((name) => !kids.some((m) => m.model === name));

    const keyField = (
        <TextField
            label="API Key"
            type="password"
            value={src.key[0] ?? ''}
            disabled={!known}
            placeholder={preset?.local ? '本机服务一般不用填' : undefined}
            hint={src.key.length > 1 ? `还有 ${src.key.length - 1} 个备用 Key，在 WebUI 里管` : undefined}
            className="font-mono"
            onValueChange={(v) => patchSrc({ key: v ? [v, ...src.key.slice(1)] : src.key.slice(1) })}
        />
    );
    const baseField = (
        <TextField
            label="接口地址"
            value={src.api_base}
            disabled={!known}
            placeholder="https://…/v1"
            className="font-mono"
            onValueChange={(api_base) => patchSrc({ api_base })}
        />
    );

    return (
        <FormDialog
            open
            size="md"
            title={
                <span className="flex min-w-0 items-center gap-2">
                    <span className="truncate">
                        {draft.index === null ? `添加 ${preset?.label ?? src.provider}` : `编辑 ${original?.id}`}
                    </span>
                    {known && <Badge tone="info">{TYPE_LABEL[src.provider_type] ?? src.provider_type}</Badge>}
                </span>
            }
            description={known ? undefined : '这类提供商的字段桌面端不认，改动请去 AstrBot WebUI'}
            headerActions={<Switch label="启用" checked={src.enable} onCheckedChange={(enable) => patchSrc({ enable })} />}
            confirmLabel={draft.index === null ? '添加' : '确定'}
            confirmDisabled={!!idError}
            onCancel={onCancel}
            onConfirm={onConfirm}
        >
            {preset?.local ? (
                <>
                    {baseField}
                    {keyField}
                </>
            ) : (
                <>
                    {keyField}
                    {baseField}
                </>
            )}

            <div>
                <button
                    type="button"
                    className="inline-flex items-center gap-1 text-xs text-text-tertiary hover:text-text"
                    aria-expanded={advOpen}
                    onClick={() => setAdv(!advOpen)}
                >
                    <ExpandChevron open={advOpen} size={12} />
                    高级
                </button>
                <ExpandPresence visible={advOpen}>
                    <div className={cn(CONFIG_PAIR, 'pt-2.5')}>
                        <TextField
                            label="标识"
                            value={src.id}
                            error={idError}
                            disabled={!known}
                            hint={isSaved ? '改了等于删旧建新，会话规则里引用的旧 id 会失效' : undefined}
                            className="font-mono"
                            onValueChange={(v) => patchSrc({ id: v })}
                        />
                        <Select
                            label="用途"
                            value={TYPE_ITEMS.some((t) => t.value === src.provider_type) ? src.provider_type : undefined}
                            items={TYPE_ITEMS}
                            disabled={!known}
                            onValueChange={(provider_type) => patchSrc({ provider_type })}
                        />
                        <TextField label="厂商" value={src.provider} disabled={!known} onValueChange={(provider) => patchSrc({ provider })} />
                        <TextField
                            label="适配器"
                            value={src.type}
                            disabled={!known}
                            className="font-mono"
                            onValueChange={(type) => patchSrc({ type })}
                        />
                        <NumberField
                            label="超时（秒）"
                            value={src.timeout}
                            min={1}
                            disabled={!known}
                            onValueChange={(timeout) => patchSrc({ timeout: timeout ?? src.timeout })}
                        />
                        <TextField
                            label="代理"
                            value={src.proxy}
                            disabled={!known}
                            placeholder="http://127.0.0.1:7890"
                            className="font-mono"
                            onValueChange={(proxy) => patchSrc({ proxy })}
                        />
                    </div>
                </ExpandPresence>
            </div>

            <div className="flex flex-col gap-2">
                <div className="flex items-center justify-between gap-2">
                    <span className="text-xs font-medium text-text-secondary">{kids.length ? `模型 · ${kids.length}` : '模型'}</span>
                    {canFetch && (
                        <Button size="sm" variant="ghost" title={fetchTitle} onClick={() => void fetchModels()}>
                            <RefreshCw size={12} className={fetching ? 'animate-spin' : undefined} />
                            拉取列表
                        </Button>
                    )}
                </div>
                {kids.length > 0 && (
                    <div className="flex flex-col divide-y divide-border-subtle/70">
                        {kids.map((m) => {
                            const isDefault = chat && draft.defaultId === m.id;
                            return (
                                <div key={m.id} className="flex items-center gap-1.5 py-1.5">
                                    {chat && (
                                        <button
                                            type="button"
                                            aria-label={isDefault ? '当前默认模型' : '设为默认模型'}
                                            aria-pressed={isDefault}
                                            title={isDefault ? '当前默认模型' : '设为默认模型'}
                                            disabled={!m.enable}
                                            onClick={() => onChange({ ...draft, defaultId: m.id })}
                                            className={cn(
                                                'flex h-7 w-7 shrink-0 items-center justify-center rounded-sm transition-colors disabled:opacity-40',
                                                isDefault ? 'text-warning' : 'text-text-disabled hover:bg-surface hover:text-text-secondary',
                                            )}
                                        >
                                            <Star size={14} fill={isDefault ? 'currentColor' : 'none'} />
                                        </button>
                                    )}
                                    <TextField
                                        value={m.model}
                                        aria-label="模型名"
                                        className="min-w-0 flex-1 font-mono"
                                        onValueChange={(model) => patchModel(m.id, { model })}
                                    />
                                    <Switch
                                        checked={m.enable}
                                        aria-label="启用模型"
                                        onCheckedChange={(enable) => patchModel(m.id, { enable })}
                                    />
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        className="h-7 w-7 text-danger hover:text-danger"
                                        aria-label="删除模型"
                                        onClick={() => removeModel(m.id)}
                                    >
                                        <Trash2 size={13} />
                                    </Button>
                                </div>
                            );
                        })}
                    </div>
                )}
                {picks && (
                    <div className="flex flex-wrap items-center gap-1.5">
                        <span className="mr-1 text-2xs text-text-tertiary">
                            {unpicked.length ? '点一下加入：' : picks.length ? '拉到的都已经加了' : '服务端没返回任何模型'}
                        </span>
                        {unpicked.map((name) => (
                            <button
                                key={name}
                                type="button"
                                onClick={() => addModel(name)}
                                className="inline-flex items-center gap-1 rounded-pill border border-border-subtle bg-surface px-2 py-0.5 font-mono text-2xs text-text-secondary transition-colors hover:border-brand hover:text-brand"
                            >
                                <Plus size={10} />
                                {name}
                            </button>
                        ))}
                    </div>
                )}
                <div className="flex items-center gap-2">
                    <TextField
                        value={manual}
                        aria-label="模型名"
                        placeholder="模型名，回车加入"
                        disabled={!known}
                        className="min-w-0 flex-1 font-mono"
                        onValueChange={setManual}
                        onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                                e.preventDefault();
                                addModel(manual);
                                setManual('');
                            }
                        }}
                    />
                    <Button
                        size="sm"
                        variant="secondary"
                        aria-label="加入模型"
                        disabled={!manual.trim() || !known}
                        onClick={() => {
                            addModel(manual);
                            setManual('');
                        }}
                    >
                        <Plus size={13} />
                    </Button>
                </div>
            </div>
        </FormDialog>
    );
};

const PresetGroup: React.FC<{
    title: string;
    items: readonly AstrBotSourcePreset[];
    onPick: (p: AstrBotSourcePreset) => void;
}> = ({ title, items, onPick }) => (
    <div>
        <p className="px-2 pb-1 pt-1.5 text-2xs font-medium uppercase tracking-wider text-text-tertiary">{title}</p>
        {items.map((p) => (
            <PopoverClose key={p.id} asChild>
                <button
                    type="button"
                    className="flex w-full items-center justify-between gap-3 rounded-sm px-2 py-1.5 text-left text-[13px] text-text hover:bg-inset"
                    onClick={() => onPick(p)}
                >
                    <span className="shrink-0 whitespace-nowrap">{p.label}</span>
                    {p.api_base && (
                        <span className="min-w-0 truncate font-mono text-2xs text-text-tertiary">{hostOf(p.api_base)}</span>
                    )}
                </button>
            </PopoverClose>
        ))}
    </div>
);
