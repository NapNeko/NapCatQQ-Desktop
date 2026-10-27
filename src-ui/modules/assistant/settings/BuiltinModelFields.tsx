// 内置模型字段；API Key 只在调用方组件内存放，随「保存」一次提交。

import { useState } from 'react';
import { ChevronDown } from 'lucide-react';
import type {
    AssistantBuiltinSettings,
    AssistantModelStatus,
} from '../../../core/domain/assistant/types';
import {
    applyAssistantModelPreset,
    applyAssistantApiFormat,
    assistantApiFormats,
    assistantModelPresets,
    modelOrigin,
    requiresBuiltinApiKey,
    validateBuiltinSettings,
} from '../../../core/domain/assistant/model';
import { Badge, Button, NumberField, Select, TextField } from '../../../shared/ui';
import { ExpandPresence } from '../../../shared/ui/motion';

interface Props {
    value: AssistantBuiltinSettings;
    savedSettings: AssistantBuiltinSettings;
    status: AssistantModelStatus | undefined;
    statusError: string | null;
    apiKey: string;
    disabled: boolean;
    // 首次配置卡片：不显示状态头、处理限制和 Key 副作用说明。
    compact?: boolean;
    onChange: (patch: Partial<AssistantBuiltinSettings>) => void;
    onApiKey: (value: string) => void;
    onClearKey?: () => void;
}

export function BuiltinModelFields({
    value,
    savedSettings,
    status,
    statusError,
    apiKey,
    disabled,
    compact = false,
    onChange,
    onApiKey,
    onClearKey,
}: Props) {
    const errors = validateBuiltinSettings(value);
    const [limitsOpen, setLimitsOpen] = useState(false);
    const [keyCleared, setKeyCleared] = useState(false);
    const endpoint = value.baseUrl.trim();
    const origin = modelOrigin(endpoint);
    // 凭据绑定 origin；同一服务下切换 API 路径不需要重复填写 Key。
    const sameOrigin = !!origin && origin === modelOrigin(savedSettings.baseUrl);
    const sameProfile =
        value.baseUrl.trim() === savedSettings.baseUrl.trim() &&
        value.model.trim() === savedSettings.model.trim() &&
        (value.providerPreset ?? 'custom') === (savedSettings.providerPreset ?? 'custom') &&
        (value.apiFormat ?? 'openAiChatCompletions') ===
            (savedSettings.apiFormat ?? 'openAiChatCompletions');
    const keySaved = !!status?.hasApiKey && sameOrigin;
    const keyUsable = !!endpoint && !errors.baseUrl;
    const requiresKey = requiresBuiltinApiKey(value);
    const configured =
        sameOrigin && sameProfile && (!requiresKey || keySaved) && !!status?.configured;
    const preset = assistantModelPresets.find(
        (item) => item.value === (value.providerPreset ?? 'custom'),
    )!;
    const changeEndpoint = (patch: Partial<AssistantBuiltinSettings>) => {
        if (patch.baseUrl !== undefined && modelOrigin(patch.baseUrl) !== origin && apiKey) {
            onApiKey('');
            setKeyCleared(true);
        }
        onChange(patch);
    };
    return (
        <div className="space-y-4">
            {!compact && (
                <div className="flex items-center gap-2">
                    <h3 className="text-sm font-medium text-text">内置助手</h3>
                    <Badge tone={configured ? 'success' : 'neutral'} dot>
                        {configured ? '已配置' : '待配置'}
                    </Badge>
                </div>
            )}
            <div className="grid grid-cols-2 gap-3">
                <Select
                    label="服务商预设"
                    value={value.providerPreset ?? 'custom'}
                    items={assistantModelPresets.map(({ value, label }) => ({ value, label }))}
                    disabled={disabled}
                    onValueChange={(providerPreset) =>
                        changeEndpoint(applyAssistantModelPreset(value, providerPreset))
                    }
                />
                <Select
                    label="API 格式"
                    value={value.apiFormat ?? 'openAiChatCompletions'}
                    items={assistantApiFormats}
                    disabled={disabled}
                    onValueChange={(apiFormat) =>
                        changeEndpoint(applyAssistantApiFormat(value, apiFormat))
                    }
                />
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
                <TextField
                    label="服务地址"
                    value={value.baseUrl}
                    placeholder={preset.baseUrl || 'https://api.example.com/v1'}
                    disabled={disabled}
                    error={errors.baseUrl}
                    onValueChange={(baseUrl) => changeEndpoint({ baseUrl })}
                />
                <TextField
                    label="模型名称"
                    value={value.model}
                    placeholder="服务提供方的模型 ID"
                    disabled={disabled}
                    onValueChange={(model) => onChange({ model })}
                />
            </div>
            {preset.models.length > 0 && (
                <div className="flex flex-wrap items-center gap-2 text-xs">
                    <span className="text-text-tertiary">模型示例</span>
                    {preset.models.map((model) => (
                        <Button
                            key={model}
                            size="sm"
                            variant="ghost"
                            disabled={disabled}
                            className="h-6 px-2 text-xs"
                            onClick={() => onChange({ model })}
                        >
                            {model}
                        </Button>
                    ))}
                </div>
            )}
            <div className="space-y-1.5">
                <TextField
                    label={requiresKey ? 'API Key' : 'API Key（可选）'}
                    type="password"
                    autoComplete="new-password"
                    value={apiKey}
                    placeholder={
                        !keyUsable
                            ? '先填写服务地址'
                            : keySaved
                              ? '已保存，输入新 Key 可替换'
                              : requiresKey
                                ? '输入 API Key'
                                : '本机 Ollama 可留空'
                    }
                    disabled={disabled || !keyUsable}
                    onValueChange={(key) => {
                        setKeyCleared(false);
                        onApiKey(key);
                    }}
                />
                {keyCleared && (
                    <p role="status" className="text-xs text-text-secondary">
                        服务地址已换源，未保存的 API Key 已清空。
                    </p>
                )}
                {!compact && (
                    <div className="flex min-h-7 flex-wrap items-center justify-between gap-x-3 gap-y-1">
                        <p className="text-2xs leading-snug text-text-tertiary">
                            保存或清除 API Key 会停止内置会话，历史会保留。
                        </p>
                        {keySaved && onClearKey && (
                            <Button
                                size="sm"
                                variant="ghost"
                                className="h-6 px-2"
                                disabled={disabled}
                                onClick={onClearKey}
                            >
                                清除 Key
                            </Button>
                        )}
                    </div>
                )}
                {sameProfile && (statusError || status?.error) && (
                    <p role="alert" className="break-words text-xs text-danger">
                        {statusError ?? status?.error}
                    </p>
                )}
            </div>
            {!compact && (
                <>
                    <button
                        type="button"
                        className="inline-flex items-center gap-1.5 rounded-xs py-1 text-xs text-text-secondary transition-colors hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                        aria-expanded={limitsOpen}
                        onClick={() => setLimitsOpen(!limitsOpen)}
                    >
                        <ChevronDown
                            size={13}
                            className={`transition-transform ${limitsOpen ? 'rotate-180' : ''}`}
                        />
                        处理限制
                    </button>
                    <ExpandPresence visible={limitsOpen} keepMounted>
                        <div className="grid gap-3 pt-1 sm:grid-cols-3">
                            <NumberField
                                label="单轮最多步骤"
                                min={1}
                                max={12}
                                value={value.maxSteps || null}
                                disabled={disabled}
                                error={errors.maxSteps}
                                onValueChange={(next) => onChange({ maxSteps: next ?? 0 })}
                            />
                            <NumberField
                                label="输出 Token 上限"
                                min={128}
                                max={8192}
                                value={value.maxOutputTokens || null}
                                disabled={disabled}
                                error={errors.maxOutputTokens}
                                onValueChange={(next) => onChange({ maxOutputTokens: next ?? 0 })}
                            />
                            <NumberField
                                label="请求超时（秒）"
                                min={10}
                                max={180}
                                value={value.requestTimeoutSecs || null}
                                disabled={disabled}
                                error={errors.requestTimeoutSecs}
                                onValueChange={(next) =>
                                    onChange({ requestTimeoutSecs: next ?? 0 })
                                }
                            />
                        </div>
                    </ExpandPresence>
                </>
            )}
        </div>
    );
}
