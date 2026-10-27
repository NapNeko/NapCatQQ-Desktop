// 新建诊断页内的首次模型配置：地址、模型与 Key 填完一次保存。

import { useEffect, useState, type ReactNode } from 'react';
import { KeyRound, Loader2, RefreshCw } from 'lucide-react';
import type { AssistantBuiltinSettings } from '../../../core/domain/assistant/types';
import {
    normalizeBuiltinSettings,
    modelOrigin,
    requiresBuiltinApiKey,
    validateBuiltinSettings,
} from '../../../core/domain/assistant/model';
import { useAssistantSettings } from '../../../hooks/assistant/useAssistant';
import { useAssistantModelSubmit } from '../../../hooks/assistant/useAssistantModelSubmit';
import { useAssistantModelKeyDraft } from '../../../hooks/assistant/useAssistantModelKey';
import { Button } from '../../../shared/ui';
import { BuiltinModelFields } from './BuiltinModelFields';

interface Props {
    footerStart?: ReactNode;
    onMore: () => void;
}

export function ModelSetup({ footerStart, onMore }: Props) {
    const { settings, modelStatus } = useAssistantSettings();
    const model = useAssistantModelSubmit();
    const [draft, setDraft] = useState<AssistantBuiltinSettings | null>(null);
    const [dirty, setDirty] = useState(false);
    const keyDraft = useAssistantModelKeyDraft(draft?.baseUrl ?? '');
    const apiKey = keyDraft.value;
    useEffect(() => {
        if (settings.data && !dirty) setDraft(normalizeBuiltinSettings(settings.data.builtin));
    }, [settings.data, dirty]);
    if (!draft || !settings.data) return null;
    const saved = settings.data.builtin;
    const next = normalizeBuiltinSettings(draft);
    const changed =
        next.baseUrl !== saved.baseUrl ||
        next.model !== saved.model ||
        next.providerPreset !== saved.providerPreset ||
        next.apiFormat !== saved.apiFormat;
    const origin = modelOrigin(next.baseUrl);
    const keySaved =
        !!modelStatus.data?.hasApiKey && !!origin && origin === modelOrigin(saved.baseUrl);
    const complete =
        !!next.baseUrl &&
        !!next.model &&
        Object.keys(validateBuiltinSettings(next)).length === 0 &&
        (!requiresBuiltinApiKey(next) || keySaved || apiKey.trim().length > 0);
    const submit = async () => {
        const key = apiKey.trim();
        const expectedOrigin = keyDraft.origin ?? '';
        keyDraft.clear();
        const result = await model.submit(
            changed ? { ...settings.data!, builtin: next } : null,
            key,
            expectedOrigin,
        );
        if (result === 'settingsFailed' && key) keyDraft.change(key, expectedOrigin);
        if (result && result !== 'settingsFailed') setDirty(false);
    };
    return (
        <form
            aria-label="连接模型"
            className="space-y-4 rounded-xl border border-border-subtle bg-surface p-5 shadow-card"
            onSubmit={(event) => {
                event.preventDefault();
                if (complete && !model.busy) void submit();
            }}
        >
            <div className="flex items-start gap-3">
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-brand-soft text-brand">
                    <KeyRound size={16} />
                </span>
                <div className="min-w-0 space-y-0.5">
                    <h3 className="text-sm font-semibold text-text">先连接一个模型</h3>
                    <p className="text-xs leading-5 text-text-tertiary">
                        选择服务商或自定义接口，Key 存在系统凭据中。
                    </p>
                </div>
            </div>
            <BuiltinModelFields
                value={draft}
                savedSettings={saved}
                status={modelStatus.data}
                statusError={modelStatus.error?.message ?? null}
                apiKey={apiKey}
                disabled={model.busy}
                compact
                onChange={(patch) => {
                    setDraft({ ...draft, ...patch });
                    setDirty(true);
                }}
                onApiKey={keyDraft.change}
            />
            {keyDraft.notice && (
                <p role="status" className="text-xs text-text-secondary">
                    {keyDraft.notice}
                </p>
            )}
            {modelStatus.isError && (
                <Button
                    size="sm"
                    variant="ghost"
                    disabled={model.busy || modelStatus.isFetching}
                    onClick={() => void modelStatus.refetch()}
                >
                    <RefreshCw
                        size={13}
                        className={modelStatus.isFetching ? 'animate-spin' : undefined}
                    />
                    重新读取凭据状态
                </Button>
            )}
            {model.error && (
                <p role="alert" className="break-words text-xs text-danger">
                    {model.error}
                </p>
            )}
            <div className="flex flex-wrap items-center gap-2 border-t border-border-subtle pt-4">
                <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1">
                    {footerStart && (
                        <fieldset disabled={model.busy} className="min-w-0">
                            {footerStart}
                        </fieldset>
                    )}
                    <Button size="sm" variant="ghost" disabled={model.busy} onClick={onMore}>
                        更多设置
                    </Button>
                </div>
                <Button
                    type="submit"
                    size="sm"
                    variant="primary"
                    disabled={!complete || model.busy}
                >
                    {model.busy && <Loader2 size={13} className="animate-spin" />}
                    保存并继续
                </Button>
            </div>
        </form>
    );
}
