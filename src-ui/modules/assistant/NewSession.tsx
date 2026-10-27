// 新建诊断：选好 Bot 与助手后直接描述问题，发送即创建会话。

import { ArrowUp, Bot, Loader2, Sparkles, Stethoscope } from 'lucide-react';
import type {
    AssistantModelStatus,
    AssistantProvider,
    AssistantProviderStatus,
    AssistantSettings,
} from '../../core/domain/assistant/types';
import { assistantStarterPrompts } from '../../core/domain/assistant/session';
import { requiresBuiltinApiKey } from '../../core/domain/assistant/model';
import { Button } from '../../shared/ui';
import { Composer } from './Composer';
import { PickerMenu, type PickerOption } from './PickerMenu';
import { ModelSetup } from './settings/ModelSetup';

interface Props {
    bots: { id: string; name: string }[];
    botId: string;
    provider: AssistantProvider | null;
    settings: AssistantSettings | undefined;
    providers: AssistantProviderStatus[];
    modelStatus: AssistantModelStatus | undefined;
    loading: boolean;
    providersLoading: boolean;
    creating: boolean;
    draft: string;
    onDraft: (value: string) => void;
    onBot: (id: string) => void;
    onProvider: (provider: AssistantProvider) => void;
    onDiscover: () => void;
    onStart: (provider: AssistantProvider, botId: string, text: string) => void;
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
    loading,
    providersLoading,
    creating,
    draft,
    onDraft,
    onBot,
    onProvider,
    onDiscover,
    onStart,
    onSettings,
    onBots,
}: Props) {
    const target = bots.some((bot) => bot.id === botId) ? botId : bots[0]?.id;
    const chosen = provider ?? settings?.defaultProvider ?? 'builtin';
    const builtin = chosen === 'builtin';
    const external = providers.filter((item) => item.provider !== 'builtin');
    const status = external.find((item) => item.provider === chosen);
    const config = settings?.providers.find((item) => item.provider === chosen);
    const nativeRequired =
        !builtin && !constrainedProviders.includes(chosen) && !config?.allowNativeTools;
    const modelReady =
        !!modelStatus?.configured &&
        !!settings &&
        (!requiresBuiltinApiKey(settings.builtin) || modelStatus.hasApiKey) &&
        !modelStatus.error &&
        !!settings?.builtin.baseUrl.trim() &&
        !!settings.builtin.model.trim();
    const ready = builtin
        ? modelReady
        : !!status?.enabled &&
          status.installed &&
          !status.error &&
          !nativeRequired &&
          !providersLoading;
    const canStart = !loading && !creating && !!target && ready;
    const start = (text: string) => {
        const question = text.trim();
        if (canStart && target && question) onStart(chosen, target, question);
    };
    const model = settings?.builtin.model.trim();
    const providerOptions: PickerOption<AssistantProvider>[] = [
        { value: 'builtin', label: '内置助手', detail: model || '待配置' },
        ...external.map((item) => ({
            value: item.provider,
            label: item.label,
            detail: !item.enabled
                ? '已停用'
                : !item.installed
                  ? '未安装'
                  : item.error
                    ? '需要检查'
                    : undefined,
            disabled: !item.enabled,
        })),
    ];
    if (!providerOptions.some((item) => item.value === chosen))
        providerOptions.push({ value: chosen, label: config?.provider ?? chosen });
    const providerText = builtin
        ? model
            ? `内置 · ${model}`
            : '内置助手'
        : (status?.label ?? chosen);
    const providerPicker = (
        <PickerMenu
            label="助手"
            icon={Sparkles}
            value={chosen}
            text={providerText}
            options={providerOptions}
            loading={providersLoading ? '检测外部 Agent…' : null}
            action={{ label: '管理模型与 Agent…', onSelect: onSettings }}
            disabled={creating}
            onChange={onProvider}
            onOpenChange={(open) => {
                if (open) onDiscover();
            }}
        />
    );
    const externalNotice = builtin ? null : providersLoading ? (
        <p role="status" className="flex items-center gap-2 text-text-secondary">
            <Loader2 size={13} className="animate-spin" />
            检测外部 Agent…
        </p>
    ) : !ready ? (
        <div className="flex flex-wrap items-center justify-between gap-2">
            <p
                role={status?.error ? 'alert' : undefined}
                className={status?.error ? 'break-words text-danger' : 'text-text-secondary'}
            >
                {status?.error ??
                    (!status?.enabled
                        ? '此 Agent 已在设置中停用。'
                        : !status.installed
                          ? '未检测到此 Agent，请在设置中检查安装或可执行文件路径。'
                          : '此 Agent 需要在设置中允许内建工具后使用。')}
            </p>
            <Button size="sm" variant="ghost" className="h-6 px-2" onClick={onSettings}>
                打开设置
            </Button>
        </div>
    ) : null;
    return (
        <div className="flex min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 py-8 sm:px-6">
            <div className="m-auto w-full max-w-2xl space-y-6 py-4">
                <header className="space-y-2 text-center">
                    <span className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl bg-brand-soft text-brand">
                        <Stethoscope size={22} strokeWidth={1.75} />
                    </span>
                    <h2 className="font-display text-lg font-semibold text-text">
                        遇到了什么问题？
                    </h2>
                    <p className="text-xs leading-5 text-text-tertiary">
                        助手会把所选 Bot 的运行状态与脱敏日志交给模型分析。
                    </p>
                </header>
                {loading ? (
                    <p
                        role="status"
                        className="flex items-center justify-center gap-2 text-sm text-text-secondary"
                    >
                        <Loader2 size={15} className="animate-spin" />
                        加载助手设置…
                    </p>
                ) : bots.length === 0 ? (
                    <div className="space-y-3 rounded-xl border border-dashed border-border p-6 text-center">
                        <p className="text-sm text-text-secondary">先添加一个 Bot，再开始诊断。</p>
                        <Button size="sm" onClick={onBots}>
                            <Bot size={14} />
                            前往机器人
                        </Button>
                    </div>
                ) : builtin && !modelReady ? (
                    <ModelSetup footerStart={providerPicker} onMore={onSettings} />
                ) : (
                    <>
                        <Composer
                            label="描述问题"
                            value={draft}
                            placeholder="例如：Bot 从今天上午开始收不到群消息"
                            autoFocus
                            disabled={creating}
                            notice={externalNotice}
                            onChange={onDraft}
                            onSubmit={() => start(draft)}
                            tools={
                                <>
                                    <PickerMenu
                                        label="目标 Bot"
                                        icon={Bot}
                                        value={target ?? ''}
                                        text={
                                            bots.find((bot) => bot.id === target)?.name ??
                                            '选择 Bot'
                                        }
                                        options={bots.map((bot) => ({
                                            value: bot.id,
                                            label: bot.name,
                                            detail: bot.id,
                                        }))}
                                        disabled={creating}
                                        onChange={onBot}
                                    />
                                    {providerPicker}
                                </>
                            }
                            action={
                                <Button
                                    size="icon"
                                    variant="primary"
                                    className="h-8 w-8 rounded-full disabled:bg-inset disabled:text-text-disabled disabled:opacity-100 disabled:shadow-none"
                                    aria-label="开始诊断"
                                    disabled={!canStart || !draft.trim()}
                                    onClick={() => start(draft)}
                                >
                                    {creating ? (
                                        <Loader2 size={14} className="animate-spin" />
                                    ) : (
                                        <ArrowUp size={15} />
                                    )}
                                </Button>
                            }
                        />
                        <div className="flex flex-wrap justify-center gap-2">
                            {assistantStarterPrompts.map((text) => (
                                <button
                                    key={text}
                                    type="button"
                                    disabled={!canStart}
                                    onClick={() => start(text)}
                                    className="rounded-pill border border-border-subtle bg-surface px-3 py-2 text-xs text-text-secondary transition-colors hover:border-border hover:bg-inset hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-50"
                                >
                                    {text}
                                </button>
                            ))}
                        </div>
                        {creating && (
                            <p
                                role="status"
                                className="flex items-center justify-center gap-2 text-xs text-text-tertiary"
                            >
                                <Loader2 size={12} className="animate-spin" />
                                正在连接助手…
                            </p>
                        )}
                    </>
                )}
            </div>
        </div>
    );
}
