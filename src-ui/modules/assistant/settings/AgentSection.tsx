// 诊断助手使用的本机 agent 设置。

import { useEffect, useState } from 'react';
import { ChevronDown, ExternalLink, Loader2, RefreshCw } from 'lucide-react';
import type {
    AssistantProvider,
    AssistantProviderConfig,
    AssistantSettings,
} from '../../../core/domain/assistant/types';
import { useAssistantSettings } from '../../../hooks/assistant/useAssistant';
import { useOpenExternal } from '../../../hooks/useOpenExternal';
import {
    Badge,
    Button,
    FormSection,
    Select,
    Switch,
    TextAreaField,
    TextField,
} from '../../../shared/ui';
import { ExpandPresence } from '../../../shared/ui/motion';
import { validateBuiltinSettings } from '../../../core/domain/assistant/model';
import { BuiltinModelFields } from './BuiltinModelFields';

export function AgentSection() {
    const [externalOpen, setExternalOpen] = useState(false);
    const { settings, providers, modelStatus, save, preview } = useAssistantSettings({
        discoverProviders: externalOpen,
    });
    const [draft, setDraft] = useState<AssistantSettings | null>(null);
    const [dirty, setDirty] = useState(false);
    const [expanded, setExpanded] = useState<AssistantProvider | null>(null);
    const openExternal = useOpenExternal();
    useEffect(() => {
        if (settings.data && !dirty) setDraft(settings.data);
    }, [settings.data, dirty]);
    const statuses = (providers.data ?? []).filter((item) => item.provider !== 'builtin');
    const changeProvider = (
        provider: AssistantProvider,
        value: Partial<AssistantProviderConfig>,
    ) => {
        if (!draft) return;
        setDirty(true);
        setDraft({
            ...draft,
            providers: draft.providers.map((item) =>
                item.provider === provider ? { ...item, ...value } : item,
            ),
        });
    };
    const failure = settings.error ?? providers.error ?? save.error;
    return (
        <FormSection
            title="诊断助手"
            description="配置内置模型，或连接已安装的外部 Agent。"
            actions={
                externalOpen ? (
                    <Button
                        size="sm"
                        variant="ghost"
                        disabled={providers.isFetching}
                        onClick={() => {
                            void providers.refetch();
                        }}
                    >
                        <RefreshCw
                            size={13}
                            className={providers.isFetching ? 'animate-spin' : undefined}
                        />
                        重新检测
                    </Button>
                ) : undefined
            }
        >
            {preview && (
                <p className="text-xs text-text-tertiary">
                    界面预览：此处设置只在当前浏览器页面生效。
                </p>
            )}
            {failure && (
                <p role="alert" className="break-words text-xs text-danger">
                    {String(failure instanceof Error ? failure.message : failure)}
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
                        savedBaseUrl={settings.data?.builtin.baseUrl ?? ''}
                        status={modelStatus.data}
                        statusError={modelStatus.error?.message ?? null}
                        disabled={save.isPending}
                        onChange={(value) => {
                            setDraft({ ...draft, builtin: { ...draft.builtin, ...value } });
                            setDirty(true);
                        }}
                    />
                    <div className="border-t border-border-subtle/70 pt-3">
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
                    </div>
                    <ExpandPresence visible={externalOpen} keepMounted>
                        <div className="space-y-4 pt-1">
                            <Select
                                label="默认助手"
                                value={draft.defaultProvider ?? 'builtin'}
                                placeholder="选择 Agent"
                                items={[
                                    { value: 'builtin' as AssistantProvider, label: '内置助手' },
                                    ...statuses.map((item) => ({
                                        value: item.provider,
                                        label: item.label,
                                        disabled: !draft.providers.find(
                                            (config) => config.provider === item.provider,
                                        )?.enabled,
                                    })),
                                ]}
                                onValueChange={(provider) => {
                                    setDraft({ ...draft, defaultProvider: provider });
                                    setDirty(true);
                                }}
                            />
                            <div>
                                <div className="flex items-center justify-between gap-3 pb-2">
                                    <span className="text-xs font-medium text-text-secondary">
                                        可用 Agent
                                    </span>
                                    <span className="text-xs text-text-tertiary">
                                        {providers.isFetching
                                            ? '正在检测…'
                                            : '启用后可在会话中选择'}
                                    </span>
                                </div>
                                <div className="divide-y divide-border-subtle border-y border-border-subtle">
                                    {draft.providers
                                        .filter((config) => config.provider !== 'builtin')
                                        .map((config) => {
                                            const status = statuses.find(
                                                (item) => item.provider === config.provider,
                                            );
                                            const label = status?.label ?? config.provider;
                                            const isExpanded = expanded === config.provider;
                                            return (
                                                <div key={config.provider} className="py-3">
                                                    <div className="flex items-center gap-3">
                                                        <Switch
                                                            aria-label={`启用 ${label}`}
                                                            checked={config.enabled}
                                                            onCheckedChange={(enabled) =>
                                                                changeProvider(config.provider, {
                                                                    enabled,
                                                                })
                                                            }
                                                        />
                                                        <button
                                                            type="button"
                                                            className="flex min-w-0 flex-1 flex-wrap items-center gap-2 rounded-xs text-left text-sm font-medium text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                                            aria-expanded={isExpanded}
                                                            onClick={() =>
                                                                setExpanded(
                                                                    isExpanded
                                                                        ? null
                                                                        : config.provider,
                                                                )
                                                            }
                                                        >
                                                            <span className="truncate">
                                                                {label}
                                                            </span>
                                                            <Badge
                                                                tone={
                                                                    status?.installed
                                                                        ? 'success'
                                                                        : 'neutral'
                                                                }
                                                            >
                                                                {status
                                                                    ? status.installed
                                                                        ? '已检测到'
                                                                        : '未安装'
                                                                    : '待检测'}
                                                            </Badge>
                                                            <ChevronDown
                                                                size={13}
                                                                className={`ml-auto shrink-0 text-text-tertiary transition-transform ${isExpanded ? 'rotate-180' : ''}`}
                                                            />
                                                        </button>
                                                        {status?.documentationUrl && (
                                                            <Button
                                                                size="icon"
                                                                variant="ghost"
                                                                className="h-7 w-7"
                                                                aria-label={`打开 ${label} 安装文档`}
                                                                onClick={() =>
                                                                    openExternal(
                                                                        status.documentationUrl,
                                                                    )
                                                                }
                                                            >
                                                                <ExternalLink size={13} />
                                                            </Button>
                                                        )}
                                                    </div>
                                                    {status?.error && (
                                                        <p className="mt-2 break-words text-xs text-danger">
                                                            {status.error}
                                                        </p>
                                                    )}
                                                    <ExpandPresence
                                                        visible={isExpanded}
                                                        keepMounted
                                                    >
                                                        <div className="mt-4 space-y-3 border-l border-border-subtle pl-4">
                                                            {status?.version && (
                                                                <p className="text-xs text-text-tertiary">
                                                                    版本 {status.version}
                                                                </p>
                                                            )}
                                                            <TextField
                                                                label="可执行文件路径"
                                                                value={config.executablePath ?? ''}
                                                                placeholder={
                                                                    status?.executablePath ??
                                                                    '自动从 PATH 查找'
                                                                }
                                                                onValueChange={(value) =>
                                                                    changeProvider(
                                                                        config.provider,
                                                                        {
                                                                            executablePath:
                                                                                value || null,
                                                                        },
                                                                    )
                                                                }
                                                            />
                                                            <TextAreaField
                                                                label="附加参数（每行一个）"
                                                                mono
                                                                minRows={2}
                                                                maxRows={6}
                                                                value={config.extraArgs.join('\n')}
                                                                onValueChange={(value) =>
                                                                    changeProvider(
                                                                        config.provider,
                                                                        {
                                                                            extraArgs:
                                                                                value.split('\n'),
                                                                        },
                                                                    )
                                                                }
                                                            />
                                                            <Switch
                                                                label="允许 Agent 内建工具"
                                                                checked={config.allowNativeTools}
                                                                onCheckedChange={(
                                                                    allowNativeTools,
                                                                ) =>
                                                                    changeProvider(
                                                                        config.provider,
                                                                        {
                                                                            allowNativeTools,
                                                                        },
                                                                    )
                                                                }
                                                            />
                                                            <p className="text-xs leading-5 text-text-secondary">
                                                                开启后，外部 Agent
                                                                可以使用自己的文件、终端等工具，可能读写本机文件或执行命令。关闭时仅支持提供收紧配置的外部
                                                                Agent。
                                                            </p>
                                                        </div>
                                                    </ExpandPresence>
                                                </div>
                                            );
                                        })}
                                </div>
                            </div>
                        </div>
                    </ExpandPresence>
                    <div className="flex flex-wrap items-center justify-end gap-3 border-t border-border-subtle/70 pt-3">
                        {dirty && (
                            <span className="text-xs text-text-tertiary">有未保存的更改</span>
                        )}
                        <div className="flex gap-2">
                            <Button
                                size="sm"
                                disabled={!dirty || save.isPending}
                                onClick={() => {
                                    setDraft(settings.data ?? null);
                                    setDirty(false);
                                    save.reset();
                                }}
                            >
                                撤销
                            </Button>
                            <Button
                                size="sm"
                                variant="primary"
                                disabled={
                                    !dirty ||
                                    save.isPending ||
                                    Object.keys(validateBuiltinSettings(draft.builtin)).length > 0
                                }
                                onClick={() => {
                                    save.mutate(
                                        {
                                            ...draft,
                                            builtin: {
                                                ...draft.builtin,
                                                baseUrl: draft.builtin.baseUrl.trim(),
                                                model: draft.builtin.model.trim(),
                                            },
                                            providers: draft.providers.map((item) => ({
                                                ...item,
                                                executablePath: item.executablePath?.trim() || null,
                                                extraArgs: item.extraArgs.filter(
                                                    (arg) => arg.trim().length > 0,
                                                ),
                                            })),
                                        },
                                        { onSuccess: () => setDirty(false) },
                                    );
                                }}
                            >
                                {save.isPending && <Loader2 size={13} className="animate-spin" />}
                                保存助手设置
                            </Button>
                        </div>
                    </div>
                </>
            )}
        </FormSection>
    );
}
