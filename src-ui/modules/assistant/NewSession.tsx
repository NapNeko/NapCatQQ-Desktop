// 新会话默认使用内置助手，外部 Agent 按需展开。

import { Bot, ChevronDown, Loader2, Settings2, Stethoscope } from 'lucide-react';
import type {
    AssistantModelStatus,
    AssistantProvider,
    AssistantProviderStatus,
    AssistantSettings,
} from '../../core/domain/assistant/types';
import { Badge, Button, Select } from '../../shared/ui';
import { ExpandPresence } from '../../shared/ui/motion';

interface Props {
    bots: { id: string; name: string }[];
    botId: string;
    provider: AssistantProvider | null;
    settings: AssistantSettings | undefined;
    providers: AssistantProviderStatus[];
    modelStatus: AssistantModelStatus | undefined;
    modelError: string | null;
    loading: boolean;
    providersLoading: boolean;
    externalOpen: boolean;
    creating: boolean;
    onBot: (id: string) => void;
    onProvider: (provider: AssistantProvider) => void;
    onExternalOpen: (open: boolean) => void;
    onCreate: (provider: AssistantProvider, botId: string) => void;
    onSettings: () => void;
    onBots: () => void;
}

const constrainedProviders: AssistantProvider[] = ['dsh', 'qwen', 'kimi', 'miniMax'];

export function NewSession({
    bots,
    botId,
    provider,
    settings,
    providers,
    modelStatus,
    modelError,
    loading,
    providersLoading,
    externalOpen,
    creating,
    onBot,
    onProvider,
    onExternalOpen,
    onCreate,
    onSettings,
    onBots,
}: Props) {
    const target = botId || bots[0]?.id;
    const chosen = provider ?? settings?.defaultProvider ?? 'builtin';
    const builtin = chosen === 'builtin';
    const external = providers.filter((item) => item.provider !== 'builtin');
    const status = external.find((item) => item.provider === chosen);
    const config = settings?.providers.find((item) => item.provider === chosen);
    const nativeRequired =
        !builtin && !constrainedProviders.includes(chosen) && !config?.allowNativeTools;
    const modelReady =
        !!modelStatus?.configured &&
        modelStatus.hasApiKey &&
        !!settings?.builtin.baseUrl.trim() &&
        !!settings.builtin.model.trim();
    const ready = builtin
        ? modelReady
        : !!status?.enabled &&
          status.installed &&
          !status.error &&
          !nativeRequired &&
          !providersLoading;
    const canCreate =
        !loading && !creating && !!target && bots.some((bot) => bot.id === target) && ready;
    return (
        <div className="flex min-h-0 flex-1 overflow-auto px-5 py-8 sm:px-10">
            <div className="m-auto w-full max-w-lg space-y-7 py-5">
                <div className="space-y-3">
                    <div className="mb-5 inline-flex rounded-xl bg-brand-soft p-3 text-brand">
                        <Stethoscope size={28} strokeWidth={1.7} />
                    </div>
                    <h1 className="font-display text-2xl font-semibold tracking-tight text-text">
                        从问题发生的地方开始
                    </h1>
                    <p className="text-sm leading-6 text-text-secondary">
                        选择一个 Bot，描述你遇到的问题。
                    </p>
                </div>
                {loading ? (
                    <p
                        role="status"
                        className="flex items-center gap-2 text-sm text-text-secondary"
                    >
                        <Loader2 size={15} className="animate-spin" />
                        加载助手设置…
                    </p>
                ) : bots.length === 0 ? (
                    <div className="space-y-3 rounded-lg bg-inset p-4">
                        <p className="text-sm text-text-secondary">先添加一个 Bot，再开始诊断。</p>
                        <Button size="sm" onClick={onBots}>
                            <Bot size={14} />
                            前往机器人
                        </Button>
                    </div>
                ) : (
                    <div className="space-y-4">
                        <Select
                            label="目标 Bot"
                            placeholder="选择 Bot"
                            value={bots.some((bot) => bot.id === target) ? target : undefined}
                            items={bots.map((bot) => ({ value: bot.id, label: bot.name }))}
                            onValueChange={onBot}
                            disabled={creating}
                        />
                        {builtin && (
                            <div className="flex items-center justify-between gap-3 rounded-lg bg-inset/60 px-3.5 py-3">
                                <div className="min-w-0 space-y-1">
                                    <div className="flex items-center gap-2 text-sm font-medium text-text">
                                        内置助手
                                        <Badge tone={modelReady ? 'success' : 'neutral'}>
                                            {modelReady ? '已配置' : '待配置'}
                                        </Badge>
                                    </div>
                                    <p className="truncate text-xs text-text-tertiary">
                                        {settings?.builtin.model || '先配置模型与 API Key'}
                                    </p>
                                </div>
                                <Button size="sm" onClick={onSettings}>
                                    <Settings2 size={13} />
                                    {modelReady ? '模型设置' : '配置模型'}
                                </Button>
                            </div>
                        )}
                        {builtin && (modelError || modelStatus?.error) && (
                            <p role="alert" className="break-words text-xs text-danger">
                                {modelError ?? modelStatus?.error}
                            </p>
                        )}
                        <button
                            type="button"
                            className="flex items-center gap-2 text-xs text-text-secondary hover:text-text focus-visible:outline-brand"
                            aria-expanded={externalOpen || !builtin}
                            disabled={creating}
                            onClick={() => {
                                if (!builtin) {
                                    onProvider('builtin');
                                    onExternalOpen(false);
                                } else onExternalOpen(!externalOpen);
                            }}
                        >
                            <ChevronDown
                                size={13}
                                className={externalOpen || !builtin ? 'rotate-180' : ''}
                            />
                            {builtin ? '使用外部 Agent' : '改用内置助手'}
                        </button>
                        <ExpandPresence visible={externalOpen || !builtin} keepMounted>
                            <div className="space-y-3 pt-1">
                                {providersLoading ? (
                                    <p
                                        role="status"
                                        className="flex items-center gap-2 text-xs text-text-secondary"
                                    >
                                        <Loader2 size={13} className="animate-spin" />
                                        检测外部 Agent…
                                    </p>
                                ) : (
                                    <Select
                                        label="外部 Agent"
                                        value={chosen}
                                        disabled={creating}
                                        items={[
                                            {
                                                value: 'builtin' as AssistantProvider,
                                                label: '内置助手',
                                            },
                                            ...external.map((item) => ({
                                                value: item.provider,
                                                label: `${item.label}${!item.enabled ? ' · 已停用' : !item.installed ? ' · 未安装' : ''}`,
                                                disabled: !item.enabled,
                                            })),
                                        ]}
                                        onValueChange={onProvider}
                                    />
                                )}
                                {!builtin && status?.error && (
                                    <p role="alert" className="break-words text-xs text-danger">
                                        {status.error}
                                    </p>
                                )}
                                {nativeRequired && (
                                    <p className="text-xs leading-5 text-text-secondary">
                                        此 Agent 需要在设置中允许内建工具后使用。
                                    </p>
                                )}
                                {!builtin &&
                                    (!status?.installed || !status?.enabled || nativeRequired) && (
                                        <Button size="sm" onClick={onSettings}>
                                            <Settings2 size={14} />
                                            配置 Agent 与查看安装文档
                                        </Button>
                                    )}
                            </div>
                        </ExpandPresence>
                        <p className="text-xs leading-5 text-text-tertiary">
                            状态与脱敏日志将交给所选模型处理。
                        </p>
                        <Button
                            variant="primary"
                            className="w-full"
                            disabled={!canCreate}
                            onClick={() => {
                                if (target) onCreate(chosen, target);
                            }}
                        >
                            {creating ? (
                                <Loader2 size={15} className="animate-spin" />
                            ) : (
                                <Stethoscope size={15} />
                            )}
                            {creating ? '正在连接…' : '开始诊断'}
                        </Button>
                    </div>
                )}
            </div>
        </div>
    );
}
