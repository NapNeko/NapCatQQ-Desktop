// 外部 Agent 的默认选择、启用状态与高级配置。

import { useState } from 'react';
import { ChevronDown, ExternalLink } from 'lucide-react';
import type {
    AssistantProvider,
    AssistantProviderConfig,
    AssistantProviderStatus,
    AssistantSettings,
} from '../../../core/domain/assistant/types';
import { useOpenExternal } from '../../../hooks/useOpenExternal';
import { Badge, Button, Select, Switch, TextAreaField, TextField } from '../../../shared/ui';
import { ExpandPresence } from '../../../shared/ui/motion';
import { cn } from '../../../shared/utils/cn';

interface Props {
    value: AssistantSettings;
    statuses: AssistantProviderStatus[];
    disabled: boolean;
    detecting: boolean;
    onChange: (next: AssistantSettings) => void;
}

export function ExternalAgentFields({ value, statuses, disabled, detecting, onChange }: Props) {
    const [expanded, setExpanded] = useState<AssistantProvider | null>(null);
    const openExternal = useOpenExternal();
    const external = statuses.filter((item) => item.provider !== 'builtin');
    const changeProvider = (
        provider: AssistantProvider,
        patch: Partial<AssistantProviderConfig>,
    ) => {
        onChange({
            ...value,
            providers: value.providers.map((item) =>
                item.provider === provider ? { ...item, ...patch } : item,
            ),
        });
    };
    return (
        <div className="space-y-4 pt-1">
            <Select
                label="默认助手"
                disabled={disabled}
                value={value.defaultProvider ?? 'builtin'}
                placeholder="选择 Agent"
                items={[
                    { value: 'builtin' as AssistantProvider, label: '内置助手' },
                    ...external.map((item) => ({
                        value: item.provider,
                        label: item.label,
                        disabled: !value.providers.find(
                            (config) => config.provider === item.provider,
                        )?.enabled,
                    })),
                ]}
                onValueChange={(defaultProvider) => onChange({ ...value, defaultProvider })}
            />
            <div>
                <div className="flex items-center justify-between gap-3 pb-2">
                    <span className="text-xs font-medium text-text-secondary">可用 Agent</span>
                    <span className="text-xs text-text-tertiary">
                        {detecting ? '正在检测…' : '启用后可在会话中选择'}
                    </span>
                </div>
                <div className="divide-y divide-border-subtle overflow-hidden rounded-lg border border-border-subtle">
                    {value.providers
                        .filter((config) => config.provider !== 'builtin')
                        .map((config) => {
                            const status = external.find(
                                (item) => item.provider === config.provider,
                            );
                            const label = status?.label ?? config.provider;
                            const isExpanded = expanded === config.provider;
                            return (
                                <div
                                    key={config.provider}
                                    className={cn(
                                        'px-3 py-2.5 transition-colors',
                                        isExpanded && 'bg-inset/40',
                                    )}
                                >
                                    <div className="flex items-center gap-3">
                                        <Switch
                                            aria-label={`启用 ${label}`}
                                            disabled={disabled}
                                            checked={config.enabled}
                                            onCheckedChange={(enabled) =>
                                                changeProvider(config.provider, { enabled })
                                            }
                                        />
                                        <button
                                            type="button"
                                            className="flex min-w-0 flex-1 flex-wrap items-center gap-2 rounded-xs text-left text-sm font-medium text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                            aria-expanded={isExpanded}
                                            onClick={() =>
                                                setExpanded(isExpanded ? null : config.provider)
                                            }
                                        >
                                            <span className="truncate">{label}</span>
                                            <Badge tone={status?.installed ? 'success' : 'neutral'}>
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
                                                    openExternal(status.documentationUrl)
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
                                    <ExpandPresence visible={isExpanded} keepMounted>
                                        <div className="mt-3 space-y-3 pb-1">
                                            {status?.version && (
                                                <p className="text-xs text-text-tertiary">
                                                    版本 {status.version}
                                                </p>
                                            )}
                                            <TextField
                                                label="可执行文件路径"
                                                disabled={disabled}
                                                value={config.executablePath ?? ''}
                                                placeholder={
                                                    status?.executablePath ?? '自动从 PATH 查找'
                                                }
                                                onValueChange={(executablePath) =>
                                                    changeProvider(config.provider, {
                                                        executablePath: executablePath || null,
                                                    })
                                                }
                                            />
                                            <TextAreaField
                                                label="附加参数（每行一个）"
                                                disabled={disabled}
                                                mono
                                                minRows={2}
                                                maxRows={6}
                                                value={config.extraArgs.join('\n')}
                                                onValueChange={(args) =>
                                                    changeProvider(config.provider, {
                                                        extraArgs: args.split('\n'),
                                                    })
                                                }
                                            />
                                            <Switch
                                                label="允许 Agent 内建工具"
                                                disabled={disabled}
                                                checked={config.allowNativeTools}
                                                onCheckedChange={(allowNativeTools) =>
                                                    changeProvider(config.provider, {
                                                        allowNativeTools,
                                                    })
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
    );
}
