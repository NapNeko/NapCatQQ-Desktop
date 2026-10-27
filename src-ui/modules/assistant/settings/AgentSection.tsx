// 诊断助手使用的模型与本机 agent 设置。

import { useEffect, useState, type ReactNode } from 'react';
import { ChevronDown, Loader2, RefreshCw } from 'lucide-react';
import type { AssistantSettings } from '../../../core/domain/assistant/types';
import { useAssistantSettings } from '../../../hooks/assistant/useAssistant';
import { useAssistantModelSubmit } from '../../../hooks/assistant/useAssistantModelSubmit';
import { useAssistantModelKeyDraft } from '../../../hooks/assistant/useAssistantModelKey';
import { Button, FormSection } from '../../../shared/ui';
import { ExpandPresence } from '../../../shared/ui/motion';
import {
    normalizeBuiltinSettings,
    modelOrigin,
    validateBuiltinSettings,
} from '../../../core/domain/assistant/model';
import { cn } from '../../../shared/utils/cn';
import { BuiltinModelFields } from './BuiltinModelFields';
import { ExternalAgentFields } from './ExternalAgentFields';

export function AgentSection({ embedded = false }: { embedded?: boolean }) {
    const [externalOpen, setExternalOpen] = useState(false);
    const { settings, providers, modelStatus, preview } = useAssistantSettings({
        discoverProviders: externalOpen,
    });
    const model = useAssistantModelSubmit();
    const [draft, setDraft] = useState<AssistantSettings | null>(null);
    const [dirty, setDirty] = useState(false);
    const keyDraft = useAssistantModelKeyDraft(draft?.builtin.baseUrl ?? '');
    const apiKey = keyDraft.value;
    const [feedback, setFeedback] = useState<string | null>(null);
    useEffect(() => {
        if (settings.data && !dirty)
            setDraft({
                ...settings.data,
                builtin: normalizeBuiltinSettings(settings.data.builtin),
            });
    }, [settings.data, dirty]);
    const edit = (next: AssistantSettings) => {
        setDraft(next);
        setDirty(true);
        setFeedback(null);
    };
    const keyTyped = apiKey.trim().length > 0;
    const changed = dirty || keyTyped;
    const invalid = !!draft && Object.keys(validateBuiltinSettings(draft.builtin)).length > 0;
    const save = async () => {
        if (!draft) return;
        const key = apiKey.trim();
        // Key 提交即清空输入框，失败也不回显到界面。
        const expectedOrigin = keyDraft.origin ?? '';
        keyDraft.clear();
        setFeedback(null);
        const result = await model.submit(
            dirty
                ? {
                      ...draft,
                      builtin: normalizeBuiltinSettings(draft.builtin),
                      providers: draft.providers.map((item) => ({
                          ...item,
                          executablePath: item.executablePath?.trim() || null,
                          extraArgs: item.extraArgs.filter((arg) => arg.trim().length > 0),
                      })),
                  }
                : null,
            key,
            expectedOrigin,
        );
        if (result === 'settingsFailed') {
            // 设置没落盘时 Key 还没交出去，放回输入框免得重填。
            if (key) keyDraft.change(key, expectedOrigin);
            return;
        }
        if (result === null) return;
        setDirty(false);
        if (result === 'keySaved') setFeedback('API Key 已保存。');
        else if (result === 'saved') setFeedback('设置已保存。');
    };
    const loadError = settings.error ?? providers.error;
    const detectButton = externalOpen && (
        <Button
            size="sm"
            variant="ghost"
            disabled={providers.isFetching}
            onClick={() => {
                void providers.refetch();
            }}
        >
            <RefreshCw size={13} className={providers.isFetching ? 'animate-spin' : undefined} />
            重新检测
        </Button>
    );
    const body: ReactNode = (
        <>
            {preview && (
                <p className="text-xs text-text-tertiary">
                    界面预览：此处设置只在当前浏览器页面生效。
                </p>
            )}
            {loadError && (
                <p role="alert" className="break-words text-xs text-danger">
                    {String(loadError instanceof Error ? loadError.message : loadError)}
                </p>
            )}
            {!draft ? (
                <div className="flex items-center gap-2 py-3 text-sm text-text-secondary">
                    {settings.isPending ? (
                        <>
                            <Loader2 size={15} className="animate-spin" />
                            加载设置…
                        </>
                    ) : (
                        <Button
                            size="sm"
                            onClick={() => {
                                void settings.refetch();
                            }}
                        >
                            重试
                        </Button>
                    )}
                </div>
            ) : (
                <>
                    <BuiltinModelFields
                        value={draft.builtin}
                        savedSettings={settings.data?.builtin ?? draft.builtin}
                        status={modelStatus.data}
                        statusError={modelStatus.error?.message ?? null}
                        apiKey={apiKey}
                        disabled={model.busy}
                        onChange={(value) =>
                            edit({ ...draft, builtin: { ...draft.builtin, ...value } })
                        }
                        onApiKey={(value) => {
                            keyDraft.change(value);
                            setFeedback(null);
                        }}
                        onClearKey={() => {
                            keyDraft.clear();
                            setFeedback(null);
                            void model
                                .clearKey(modelOrigin(settings.data?.builtin.baseUrl ?? '') ?? '')
                                .then((cleared) => {
                                    if (cleared) setFeedback('API Key 已清除。');
                                });
                        }}
                    />
                    {keyDraft.notice && (
                        <p role="status" className="text-xs text-text-secondary">
                            {keyDraft.notice}
                        </p>
                    )}
                    <div className="border-t border-border-subtle/70 pt-3">
                        <div className="flex items-center justify-between gap-2">
                            <button
                                type="button"
                                className="inline-flex items-center gap-2 rounded-xs py-1 text-sm font-medium text-text-secondary transition-colors hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                aria-expanded={externalOpen}
                                onClick={() => setExternalOpen(!externalOpen)}
                            >
                                <ChevronDown
                                    size={14}
                                    className={`transition-transform ${externalOpen ? 'rotate-180' : ''}`}
                                />
                                外部 Agent（可选）
                            </button>
                            {embedded && detectButton}
                        </div>
                    </div>
                    <ExpandPresence visible={externalOpen} keepMounted>
                        <ExternalAgentFields
                            value={draft}
                            statuses={providers.data ?? []}
                            disabled={model.busy}
                            detecting={providers.isFetching}
                            onChange={edit}
                        />
                    </ExpandPresence>
                    <div
                        className={cn(
                            'flex flex-wrap items-center justify-end gap-3 border-t border-border-subtle/70 pt-3',
                            embedded && 'sticky bottom-0 bg-elevated pb-1',
                        )}
                    >
                        {model.error ? (
                            <p
                                role="alert"
                                className="min-w-0 flex-1 break-words text-xs text-danger"
                            >
                                {model.error}
                            </p>
                        ) : feedback ? (
                            <p role="status" className="min-w-0 flex-1 text-xs text-success">
                                {feedback}
                            </p>
                        ) : (
                            changed && (
                                <span className="min-w-0 flex-1 text-xs text-text-tertiary">
                                    有未保存的更改
                                </span>
                            )
                        )}
                        <div className="flex gap-2">
                            <Button
                                size="sm"
                                disabled={!changed || model.busy}
                                onClick={() => {
                                    setDraft(settings.data ?? null);
                                    setDirty(false);
                                    keyDraft.clear();
                                    setFeedback(null);
                                    model.reset();
                                }}
                            >
                                撤销
                            </Button>
                            <Button
                                size="sm"
                                variant="primary"
                                disabled={!changed || model.busy || invalid}
                                onClick={() => {
                                    void save();
                                }}
                            >
                                {model.busy && <Loader2 size={13} className="animate-spin" />}
                                保存助手设置
                            </Button>
                        </div>
                    </div>
                </>
            )}
        </>
    );
    if (embedded) return <div className="flex flex-col gap-4">{body}</div>;
    return (
        <FormSection
            title="诊断助手"
            description="配置内置模型，或连接已安装的外部 Agent。"
            actions={detectButton || undefined}
        >
            {body}
        </FormSection>
    );
}
