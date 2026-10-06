// 模型页：顶部定默认模型和备用模型（整个详情只有这里能改），下面是提供商列表。
// 列表一行一个提供商（id / 类型 / 地址 / 模型药丸），改什么都进对话框。

import { Boxes, ChevronDown, Plus, Star, Trash2 } from 'lucide-react';
import { Badge, Button, FormSection, Select, Switch } from '../../../../shared/ui';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import { EmptyHint, EntityRow, PickList, Pill } from './parts';
import { ProviderDialog } from './ProviderDialog';
import {
    PROVIDER_TYPE_LABEL,
    ProviderPresetMenu,
    hostOf,
    useProviderEditor,
} from './providerEditor';
import { removeProviderSource } from './providerDraft';
import {
    enabledChatModels,
    isChatProviderType,
    isKnownProviderType,
    presetLabel,
} from '../../../../core/domain/apps/astrbotConfig';
import { cn } from '../../../../shared/utils/cn';
import type {
    AstrBotAiSettings,
    AstrBotInstanceConfig,
    AstrBotProviderSource,
} from '../../../../core/ipc/types';

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
    const editor = useProviderEditor(config, onChange);
    const savedIds = new Set((saved?.sources ?? []).map((s) => s.id));
    const chat = enabledChatModels(config);
    const modelItems = chat.map((m) => ({ value: m.id, label: m.model || m.id }));
    const setAi = (patch: Partial<AstrBotAiSettings>) =>
        onChange({ ...config, ai: { ...config.ai, ...patch } });

    const toggleSource = (index: number, enable: boolean) =>
        onChange({
            ...config,
            sources: config.sources.map((s, i) => (i === index ? { ...s, enable } : s)),
        });

    const rowHasError = (index: number, src: AstrBotProviderSource) =>
        Object.keys(errors).some((k) => {
            if (k.startsWith(`sources/${index}/`)) return true;
            const m = /^models\/(\d+)\//.exec(k);
            return m ? config.models[Number(m[1])]?.provider_source_id === src.id : false;
        });

    const addMenu = (
        <ProviderPresetMenu onPick={editor.openCreate}>
            <Button size="sm" variant="secondary" disabled={disabled}>
                <Plus size={13} /> 添加提供商{' '}
                <ChevronDown size={12} className="-mr-0.5 opacity-70" />
            </Button>
        </ProviderPresetMenu>
    );

    return (
        <ConfigForm>
            {config.sources.length > 0 && (
                <FormSection title="对话模型">
                    <div className={CONFIG_PAIR}>
                        <Select
                            label="默认模型"
                            value={
                                modelItems.some((i) => i.value === config.ai.default_provider_id)
                                    ? config.ai.default_provider_id
                                    : undefined
                            }
                            items={modelItems}
                            placeholder={chat.length ? '选择模型' : '还没有可用的对话模型'}
                            disabled={disabled || chat.length === 0}
                            hint={
                                chat.length === 0
                                    ? '给下面的提供商加一个模型，并确认提供商和模型都是启用的'
                                    : undefined
                            }
                            onValueChange={(default_provider_id) => setAi({ default_provider_id })}
                        />
                    </div>
                    <PickList
                        label="备用模型"
                        options={modelItems.filter(
                            (m) => m.value !== config.ai.default_provider_id,
                        )}
                        value={config.ai.fallback_chat_models}
                        disabled={disabled}
                        empty={
                            chat.length <= 1
                                ? '至少要有两个对话模型才谈得上备用'
                                : '没有可选的备用模型'
                        }
                        hint={
                            config.ai.fallback_chat_models.length
                                ? '默认模型出错时按顺序换用'
                                : undefined
                        }
                        onChange={(fallback_chat_models) => setAi({ fallback_chat_models })}
                    />
                </FormSection>
            )}

            <FormSection
                title="模型提供商"
                actions={config.sources.length > 0 ? addMenu : undefined}
                layout="none"
            >
                {config.sources.length === 0 ? (
                    <EmptyHint
                        icon={Boxes}
                        title="还没有模型提供商。选一家，填上 API Key 就能开始"
                        action={addMenu}
                    />
                ) : (
                    <div className="flex flex-col gap-2">
                        {config.sources.map((src, i) => {
                            const known = isKnownProviderType(src.provider_type);
                            const kids = config.models.filter(
                                (m) => m.provider_source_id === src.id,
                            );
                            const isChat = isChatProviderType(src.provider_type);
                            return (
                                <EntityRow
                                    key={`${src.id}-${i}`}
                                    icon={Boxes}
                                    muted={!src.enable}
                                    title={<span className="font-mono">{src.id || '未命名'}</span>}
                                    tags={
                                        <>
                                            <Badge tone={known ? 'info' : 'neutral'}>
                                                {known
                                                    ? (PROVIDER_TYPE_LABEL[src.provider_type] ??
                                                      src.provider_type)
                                                    : '桌面端只读'}
                                            </Badge>
                                            {!savedIds.has(src.id) && (
                                                <Badge tone="warning">未保存</Badge>
                                            )}
                                            {rowHasError(i, src) && (
                                                <Badge tone="danger">有误</Badge>
                                            )}
                                        </>
                                    }
                                    subtitle={[
                                        presetLabel(src),
                                        hostOf(src.api_base) || '接口地址未填',
                                    ]
                                        .filter(Boolean)
                                        .join(' · ')}
                                    extra={
                                        kids.length ? (
                                            <span className="flex flex-wrap gap-1">
                                                {kids.map((m) => {
                                                    const isDefault =
                                                        isChat &&
                                                        m.id === config.ai.default_provider_id;
                                                    return (
                                                        <Pill
                                                            key={m.id}
                                                            className={cn(
                                                                'gap-1',
                                                                isDefault &&
                                                                    'bg-warning-soft text-warning',
                                                                !m.enable &&
                                                                    'line-through opacity-60',
                                                            )}
                                                        >
                                                            {isDefault && (
                                                                <Star
                                                                    size={9}
                                                                    fill="currentColor"
                                                                />
                                                            )}
                                                            {m.model || m.id}
                                                        </Pill>
                                                    );
                                                })}
                                            </span>
                                        ) : (
                                            <span className="text-2xs text-text-tertiary">
                                                没有模型，点进去添加
                                            </span>
                                        )
                                    }
                                    actions={
                                        <>
                                            <Switch
                                                checked={src.enable}
                                                aria-label="启用提供商"
                                                disabled={disabled}
                                                onCheckedChange={(enable) =>
                                                    toggleSource(i, enable)
                                                }
                                            />
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                className="h-7 w-7 text-danger hover:text-danger"
                                                aria-label="删除提供商"
                                                title={
                                                    kids.length
                                                        ? `连同 ${kids.length} 个模型一起移出草稿，保存前可撤销`
                                                        : undefined
                                                }
                                                disabled={disabled}
                                                onClick={() =>
                                                    onChange(removeProviderSource(config, i))
                                                }
                                            >
                                                <Trash2 size={14} />
                                            </Button>
                                        </>
                                    }
                                    onOpen={() => editor.openEdit(i)}
                                />
                            );
                        })}
                    </div>
                )}
            </FormSection>

            {editor.draft && (
                <ProviderDialog
                    draft={editor.draft}
                    config={config}
                    savedIds={savedIds}
                    running={running}
                    instanceId={instanceId}
                    errors={errors}
                    onChange={editor.setDraft}
                    onCancel={editor.cancel}
                    onConfirm={editor.confirm}
                />
            )}
        </ConfigForm>
    );
};
