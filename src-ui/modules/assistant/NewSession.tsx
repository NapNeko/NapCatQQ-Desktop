// 新会话默认使用内置助手，外部 Agent 按需展开。

import { Bot, ChevronDown, Loader2, Settings2, Stethoscope } from 'lucide-react';
import { useId } from 'react';
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
    const externalPanelId = useId();
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
        <div className="flex min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 py-6 sm:px-5 sm:py-8">
            <div className="m-auto w-full max-w-sm space-y-5 py-2">
                <div className="space-y-2 text-center">
                    <span className="mx-auto mb-3 flex h-14 w-14 items-center justify-center rounded-full bg-brand-soft/50 text-brand">
                        <Stethoscope size={26} strokeWidth={1.5} />
                    </span>
                    <h2 className="text-base font-semibold text-text-secondary">新建诊断</h2>
                    <p className="text-xs leading-5 text-text-tertiary">
                        选择诊断目标和助手，开始后即可描述问题。
                    </p>
                </div>
                {loading ? (
                    <p
                        role="status"
                        className="flex items-center justify-center gap-2 text-sm text-text-secondary"
                    >
                        <Loader2 size={15} className="animate-spin" />
                        加载助手设置…
                    </p>
                ) : bots.length === 0 ? (
                    <div className="space-y-3 text-center">
                        <p className="text-sm text-text-secondary">先添加一个 Bot，再开始诊断。</p>
                        <Button size="sm" onClick={onBots}>
                            <Bot size={14} />
                            前往机器人
                        </Button>
                    </div>
                ) : (
                    <>
                        <Select
                            label="目标 Bot"
                            placeholder="选择 Bot"
                            value={bots.some((bot) => bot.id === target) ? target : undefined}
                            items={bots.map((bot) => ({ value: bot.id, label: bot.name }))}
                            onValueChange={onBot}
                            disabled={creating}
                        />
                        <div className="space-y-3">
                            <div className="flex flex-wrap items-start justify-between gap-3">
                                <div className="min-w-0 flex-1 space-y-1">
                                    <div className="flex items-center gap-2 text-sm font-medium text-text">
                                        {builtin ? '内置助手' : '外部 Agent'}
                                        {builtin && (
                                            <Badge tone={modelReady ? 'success' : 'neutral'}>
                                                {modelReady ? '已配置' : '待配置'}
                                            </Badge>
                                        )}
                                    </div>
                                    {builtin && (
                                        <p className="break-words text-xs leading-5 text-text-tertiary">
                                            {settings?.builtin.model || '先配置模型与 API Key'}
                                        </p>
                                    )}
                                </div>
                                {builtin && (
                                    <Button size="sm" disabled={creating} onClick={onSettings}>
                                        <Settings2 size={13} />
                                        {modelReady ? '模型设置' : '配置模型'}
                                    </Button>
                                )}
                            </div>
                            {builtin && (modelError || modelStatus?.error) && (
                                <p role="alert" className="break-words text-xs text-danger">
                                    {modelError ?? modelStatus?.error}
                                </p>
                            )}
                            <button
                                type="button"
                                className="flex items-center gap-1.5 rounded-sm py-1 text-xs text-text-secondary transition-colors hover:text-text focus-visible:outline-brand disabled:opacity-50"
                                aria-expanded={externalOpen || !builtin}
                                aria-controls={externalPanelId}
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
                                    className={`transition-transform ${externalOpen || !builtin ? 'rotate-180' : ''}`}
                                />
                                {builtin ? '使用外部 Agent' : '改用内置助手'}
                            </button>
                            <ExpandPresence visible={externalOpen || !builtin} keepMounted>
                                <div id={externalPanelId} className="space-y-3 pb-1">
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
                                    {!builtin &&
                                        !providersLoading &&
                                        !status?.error &&
                                        !status?.installed && (
                                            <p className="text-xs leading-5 text-text-secondary">
                                                未检测到此
                                                Agent，请在设置中检查安装或可执行文件路径。
                                            </p>
                                        )}
                                    {nativeRequired && !providersLoading && (
                                        <p className="text-xs leading-5 text-text-secondary">
                                            此 Agent 需要在设置中允许内建工具后使用。
                                        </p>
                                    )}
                                    {!builtin &&
                                        (!status?.installed ||
                                            !status?.enabled ||
                                            nativeRequired) && (
                                            <Button size="sm" onClick={onSettings}>
                                                <Settings2 size={14} />
                                                配置 Agent 与查看安装文档
                                            </Button>
                                        )}
                                </div>
                            </ExpandPresence>
                        </div>
                        <div className="flex flex-wrap items-center justify-between gap-3">
                            <p className="min-w-0 flex-1 text-xs leading-5 text-text-tertiary">
                                状态与脱敏日志将交给所选模型处理。
                            </p>
                            <Button
                                size="sm"
                                variant="primary"
                                disabled={!canCreate}
                                onClick={() => {
                                    if (target) onCreate(chosen, target);
                                }}
                            >
                                {creating ? (
                                    <Loader2 size={14} className="animate-spin" />
                                ) : (
                                    <Stethoscope size={14} />
                                )}
                                {creating ? '正在连接…' : '开始诊断'}
                            </Button>
                        </div>
                    </>
                )}
            </div>
        </div>
    );
}
