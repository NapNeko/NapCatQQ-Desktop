// 编辑一个提供商：Key / 地址 / 高级字段 + 名下模型。对话框里是局部草稿，确定才写回表单；
// 拉取模型只对服务端已认（已保存）的源开放。默认模型不在这里选，统一去模型页顶部。

import { useState } from 'react';
import { Plus, RefreshCw, Trash2 } from 'lucide-react';
import { Badge, Button, NumberField, Select, Switch, TextField } from '../../../../shared/ui';
import { ExpandChevron, ExpandPresence } from '../../../../shared/ui/motion';
import { CONFIG_PAIR } from '../karin/configLayout';
import { FormDialog } from './parts';
import { PROVIDER_TYPE_LABEL } from './providerEditor';
import type { ProviderDraft } from './providerDraft';
import {
    ASTRBOT_PROVIDER_TYPES,
    ASTRBOT_SOURCE_PRESETS,
    isChatProviderType,
    isKnownProviderType,
    newOpenAiModel,
    uniqueId,
} from '../../../../core/domain/apps/astrbotConfig';
import { useAstrBotSourceModels } from '../../../../hooks/apps/useAstrBotDashboard';
import { cn } from '../../../../shared/utils/cn';
import type { AstrBotInstanceConfig, AstrBotProviderModel, AstrBotProviderSource } from '../../../../core/ipc/types';

const TYPE_ITEMS = ASTRBOT_PROVIDER_TYPES.map((v) => ({ value: v, label: PROVIDER_TYPE_LABEL[v] ?? v }));

export const ProviderDialog: React.FC<{
    draft: ProviderDraft;
    config: AstrBotInstanceConfig;
    /** 最近一次保存成功的源 id；没在里面的服务端不认，拉不了模型 */
    savedIds: ReadonlySet<string>;
    running: boolean;
    instanceId: string;
    errors: Record<string, string>;
    onChange: (d: ProviderDraft) => void;
    onCancel: () => void;
    onConfirm: () => void;
}> = ({ draft, config, savedIds, running, instanceId, errors, onChange, onCancel, onConfirm }) => {
    const [picks, setPicks] = useState<string[] | null>(null);
    const listModels = useAstrBotSourceModels(instanceId);
    const fetching = listModels.isPending;
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
    const fetchModels = () => {
        if (original) listModels.mutate(original.id, { onSuccess: setPicks });
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
                    {known && <Badge tone="info">{PROVIDER_TYPE_LABEL[src.provider_type] ?? src.provider_type}</Badge>}
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
                    {original && known && (
                        <Button
                            size="sm"
                            variant="ghost"
                            disabled={!canFetch}
                            title={fetchTitle}
                            onClick={fetchModels}
                        >
                            <RefreshCw size={12} className={fetching ? 'animate-spin' : undefined} />
                            拉取列表
                        </Button>
                    )}
                </div>
                {kids.length > 0 && (
                    <div className="flex flex-col divide-y divide-border-subtle/70">
                        {kids.map((m) => (
                            <div key={m.id} className="flex items-center gap-1.5 py-1.5">
                                <TextField
                                    value={m.model}
                                    aria-label="模型名"
                                    className="min-w-0 flex-1 font-mono"
                                    onValueChange={(model) => patchModel(m.id, { model })}
                                />
                                {chat && draft.defaultId === m.id && (
                                    <Badge tone="warning" className="shrink-0">
                                        默认
                                    </Badge>
                                )}
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
                        ))}
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
