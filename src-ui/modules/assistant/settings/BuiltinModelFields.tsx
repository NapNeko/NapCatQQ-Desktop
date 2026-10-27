// 模型配置与仅保留在当前组件内的 API Key 输入。

import { useEffect, useState } from 'react';
import { ChevronDown, Loader2 } from 'lucide-react';
import type {
    AssistantBuiltinSettings,
    AssistantModelStatus,
} from '../../../core/domain/assistant/types';
import { validateBuiltinSettings } from '../../../core/domain/assistant/model';
import { useAssistantModelKey } from '../../../hooks/assistant/useAssistantModelKey';
import { Badge, Button, NumberField, TextField } from '../../../shared/ui';
import { ExpandPresence } from '../../../shared/ui/motion';

interface Props {
    value: AssistantBuiltinSettings;
    savedBaseUrl: string;
    status: AssistantModelStatus | undefined;
    statusError: string | null;
    disabled: boolean;
    onChange: (patch: Partial<AssistantBuiltinSettings>) => void;
}

export function BuiltinModelFields({
    value,
    savedBaseUrl,
    status,
    statusError,
    disabled,
    onChange,
}: Props) {
    const errors = validateBuiltinSettings(value);
    const [limitsOpen, setLimitsOpen] = useState(false);
    return (
        <div className="space-y-4">
            <div className="flex items-center gap-2">
                <h3 className="text-sm font-medium text-text">内置助手</h3>
                <Badge tone={status?.configured ? 'success' : 'neutral'}>
                    {status?.configured ? '已配置' : '待配置'}
                </Badge>
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
                <TextField
                    label="服务地址"
                    value={value.baseUrl}
                    placeholder="https://…/v1"
                    disabled={disabled}
                    error={errors.baseUrl}
                    onValueChange={(baseUrl) => onChange({ baseUrl })}
                />
                <TextField
                    label="模型名称"
                    value={value.model}
                    placeholder="填写服务提供方的模型 ID"
                    disabled={disabled}
                    onValueChange={(model) => onChange({ model })}
                />
            </div>
            <ModelKeyField
                key={savedBaseUrl}
                savedBaseUrl={savedBaseUrl}
                draftBaseUrl={value.baseUrl}
                validEndpoint={!errors.baseUrl}
                status={status}
                statusError={statusError}
                disabled={disabled}
            />
            <button
                type="button"
                className="inline-flex items-center gap-2 rounded-xs py-1 text-xs text-text-secondary transition-colors hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
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
                <div className="grid gap-3 pt-3 sm:grid-cols-3">
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
                        onValueChange={(next) => onChange({ requestTimeoutSecs: next ?? 0 })}
                    />
                </div>
            </ExpandPresence>
        </div>
    );
}

interface KeyProps {
    savedBaseUrl: string;
    draftBaseUrl: string;
    validEndpoint: boolean;
    status: AssistantModelStatus | undefined;
    statusError: string | null;
    disabled: boolean;
}

function ModelKeyField({
    savedBaseUrl,
    draftBaseUrl,
    validEndpoint,
    status,
    statusError,
    disabled,
}: KeyProps) {
    const keys = useAssistantModelKey();
    const [apiKey, setApiKey] = useState('');
    const [feedback, setFeedback] = useState<string | null>(null);
    const endpointSaved =
        !!savedBaseUrl.trim() && savedBaseUrl.trim() === draftBaseUrl.trim() && validEndpoint;
    useEffect(() => {
        setApiKey('');
        setFeedback(null);
    }, [savedBaseUrl, draftBaseUrl]);
    return (
        <div className="space-y-2">
            <TextField
                label="API Key"
                type="password"
                autoComplete="new-password"
                value={apiKey}
                placeholder={status?.hasApiKey ? '已保存，可输入新 Key 替换' : '输入 API Key'}
                disabled={disabled || keys.busy || !endpointSaved}
                onValueChange={setApiKey}
            />
            <div className="flex flex-wrap items-center gap-2">
                <Button
                    size="sm"
                    disabled={!endpointSaved || !apiKey.trim() || keys.busy || disabled}
                    onClick={() => {
                        const submitted = apiKey.trim();
                        setApiKey('');
                        void keys.save(submitted).then((saved) => {
                            if (saved) setFeedback('API Key 已保存。');
                        });
                    }}
                >
                    {keys.busy && <Loader2 size={12} className="animate-spin" />}保存 Key
                </Button>
                <Button
                    size="sm"
                    variant="ghost"
                    disabled={!endpointSaved || !status?.hasApiKey || keys.busy || disabled}
                    onClick={() => {
                        setApiKey('');
                        void keys.clear().then((cleared) => {
                            if (cleared) setFeedback('API Key 已清除。');
                        });
                    }}
                >
                    清除 Key
                </Button>
                <span className="text-xs text-text-tertiary">
                    {!endpointSaved ? '服务地址待保存' : status?.hasApiKey ? '已保存' : '未配置'}
                </span>
            </div>
            <p className="text-xs text-text-secondary">
                保存或清除 API Key 会停止内置会话，历史会保留。
            </p>
            {!endpointSaved && (
                <p className="text-xs text-text-secondary">
                    先保存服务地址，再保存对应的 API Key。
                </p>
            )}
            {(keys.error || statusError || status?.error) && (
                <p role="alert" className="break-words text-xs text-danger">
                    {keys.error ?? statusError ?? status?.error}
                </p>
            )}
            {feedback && (
                <p role="status" className="text-xs text-text-secondary">
                    {feedback}
                </p>
            )}
        </div>
    );
}
