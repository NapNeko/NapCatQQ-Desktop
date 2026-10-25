// 诊断助手工作区：会话列表、目标与对话。

import { useEffect, useMemo, useState } from 'react';
import { Loader2, MessageSquare, Plus, RefreshCw, Settings2, Trash2, X } from 'lucide-react';
import type { AssistantProvider } from '../../core/domain/assistant/types';
import type { AppRoute } from '../../core/domain/ui/route';
import { assistantStateLabel, isAssistantBusy } from '../../core/domain/assistant/session';
import { useAssistant, useAssistantSettings } from '../../hooks/assistant/useAssistant';
import { consumeAssistantTarget, useAssistantTarget } from '../../hooks/assistant/assistantNav';
import { useBotSnapshots } from '../../hooks/bot/useBotSnapshots';
import { useBotConfigsMap } from '../../hooks/bot/useBotConfigsMap';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { Conversation } from './Conversation';
import { NewSession } from './NewSession';
import { AgentSection } from './settings/AgentSection';
import { MiniMaxLogin } from './MiniMaxLogin';

export function AssistantPage({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
    const assistant = useAssistant();
    const pendingTarget = useAssistantTarget();
    const botQuery = useBotSnapshots();
    const snapshots = useMemo(() => botQuery.data ?? [], [botQuery.data]);
    const configs = useBotConfigsMap(snapshots);
    const [targetBot, setTargetBot] = useState(pendingTarget ?? '');
    const [provider, setProvider] = useState<AssistantProvider | null>(null);
    const [settingsOpen, setSettingsOpen] = useState(false);
    const [showSessions, setShowSessions] = useState(false);
    const [deleteId, setDeleteId] = useState<string | null>(null);
    const [externalOpen, setExternalOpen] = useState(false);
    const info = assistant.infos.find((item) => item.id === assistant.selectedId);
    const session = assistant.selectedId ? assistant.sessions[assistant.selectedId] : undefined;
    const discoverProviders =
        externalOpen ||
        (provider !== null && provider !== 'builtin') ||
        (!!info && info.provider !== 'builtin');
    const preferences = useAssistantSettings({ discoverProviders });
    useEffect(() => {
        if (
            provider === null &&
            preferences.settings.data?.defaultProvider &&
            preferences.settings.data.defaultProvider !== 'builtin'
        )
            setExternalOpen(true);
    }, [provider, preferences.settings.data?.defaultProvider]);
    useEffect(() => {
        if (!pendingTarget) return;
        const target = consumeAssistantTarget();
        if (target) {
            setTargetBot(target);
            assistant.actions.select(null);
            setShowSessions(false);
        }
    }, [pendingTarget, assistant.actions]);
    const bots = useMemo(
        () =>
            snapshots.map((bot) => ({
                id: bot.bot_id,
                name: configs[bot.bot_id]?.bot.name || `Bot ${bot.bot_id}`,
            })),
        [snapshots, configs],
    );
    const providerLabel = (value: AssistantProvider) =>
        value === 'builtin'
            ? `内置助手${preferences.settings.data?.builtin.model ? ` · ${preferences.settings.data.builtin.model}` : ''}`
            : (preferences.providers.data?.find((item) => item.provider === value)?.label ?? value);
    const newSession = () => {
        if (info) {
            setTargetBot(info.botId);
            setProvider(info.provider);
        }
        assistant.actions.select(null);
        setShowSessions(false);
    };
    const queryError =
        preferences.settings.error ??
        (discoverProviders ? preferences.providers.error : null) ??
        botQuery.error;
    const error =
        assistant.error ??
        (queryError
            ? queryError instanceof Error
                ? queryError.message
                : String(queryError)
            : null);
    return (
        <div className="relative flex min-h-0 flex-1 flex-col gap-3">
            <div className="flex shrink-0 items-center justify-between gap-3">
                <div className="flex min-w-0 items-center gap-2.5">
                    <Button
                        size="icon"
                        variant="ghost"
                        className="md:hidden"
                        aria-label="显示诊断会话列表"
                        onClick={() => setShowSessions(!showSessions)}
                    >
                        <MessageSquare size={17} />
                    </Button>
                    <h1 className="truncate font-display text-xl font-semibold tracking-tight text-text">
                        诊断助手
                    </h1>
                    {assistant.preview && <Badge tone="warning">界面预览</Badge>}
                </div>
                <div className="flex shrink-0 gap-1">
                    <Button
                        size="icon"
                        variant="ghost"
                        aria-label="刷新诊断会话"
                        disabled={assistant.loading}
                        onClick={() => {
                            void assistant.actions.refresh();
                            void preferences.modelStatus.refetch();
                            if (discoverProviders) void preferences.providers.refetch();
                        }}
                    >
                        <RefreshCw
                            size={16}
                            className={assistant.loading ? 'animate-spin' : undefined}
                        />
                    </Button>
                    <Button
                        size="sm"
                        variant="ghost"
                        aria-label="助手设置"
                        onClick={() => setSettingsOpen(true)}
                    >
                        <Settings2 size={15} />
                        <span className="hidden sm:inline">助手设置</span>
                    </Button>
                </div>
            </div>
            {assistant.preview && (
                <p className="shrink-0 text-xs text-text-tertiary">
                    当前为浏览器界面演示，不会调用真实模型或操作 Bot。
                </p>
            )}
            {(error || assistant.notices.length > 0) && (
                <div className="shrink-0 space-y-2 rounded-lg border border-warning/20 bg-warning-soft/50 px-3 py-2 text-xs leading-5">
                    {error && (
                        <div className="flex items-start gap-3 text-danger">
                            <p role="alert" className="min-w-0 flex-1 break-words">
                                {error}
                            </p>
                            <Button
                                size="sm"
                                onClick={() => {
                                    void assistant.actions.refresh();
                                    void botQuery.refetch();
                                    void preferences.settings.refetch();
                                    void preferences.modelStatus.refetch();
                                    if (discoverProviders) void preferences.providers.refetch();
                                }}
                            >
                                重试
                            </Button>
                        </div>
                    )}
                    {assistant.notices.map((notice, index) => (
                        <p key={index} className="break-words text-text-secondary">
                            {notice}
                        </p>
                    ))}
                </div>
            )}
            <div className="relative flex min-h-0 flex-1 overflow-hidden rounded-xl border border-border-subtle bg-surface shadow-card">
                <aside
                    aria-label="诊断会话列表"
                    className={cn(
                        'absolute inset-0 z-20 flex-col border-r border-border-subtle bg-surface md:static md:z-auto md:w-56 md:shrink-0 lg:w-64',
                        showSessions ? 'flex' : 'hidden md:flex',
                    )}
                >
                    <div className="flex shrink-0 items-center gap-2 border-b border-border-subtle px-3 py-3">
                        <Button size="sm" className="flex-1" onClick={newSession}>
                            <Plus size={14} />
                            新建会话
                        </Button>
                        <Button
                            size="icon"
                            variant="ghost"
                            className="h-7 w-7 md:hidden"
                            aria-label="收起会话列表"
                            onClick={() => setShowSessions(false)}
                        >
                            <X size={15} />
                        </Button>
                    </div>
                    <nav
                        className="min-h-0 flex-1 space-y-1 overflow-y-auto p-2"
                        aria-label="选择会话"
                    >
                        {assistant.infos.length === 0 && (
                            <p className="px-3 py-6 text-center text-xs leading-5 text-text-tertiary">
                                {assistant.loading ? '加载会话…' : '开始诊断后，会话会保留在这里。'}
                            </p>
                        )}
                        {assistant.infos.map((item) => (
                            <button
                                key={item.id}
                                type="button"
                                aria-current={item.id === assistant.selectedId ? 'page' : undefined}
                                onClick={() => {
                                    assistant.actions.select(item.id);
                                    setShowSessions(false);
                                }}
                                className={cn(
                                    'w-full space-y-2 rounded-lg px-3 py-3 text-left transition-colors focus-visible:outline-brand',
                                    item.id === assistant.selectedId
                                        ? 'bg-brand-soft'
                                        : 'hover:bg-inset',
                                )}
                            >
                                <div className="truncate text-sm font-medium text-text">
                                    {item.title || '新诊断'}
                                </div>
                                <div className="flex min-w-0 items-center gap-2 text-2xs text-text-tertiary">
                                    <span className="truncate">{item.botName}</span>
                                    <span className="ml-auto shrink-0">
                                        {assistantStateLabel[item.state]}
                                    </span>
                                </div>
                                <div className="flex items-center justify-between gap-2 text-2xs text-text-tertiary">
                                    <span className="truncate">{providerLabel(item.provider)}</span>
                                    <time dateTime={new Date(item.updatedAtMs).toISOString()}>
                                        {new Date(item.updatedAtMs).toLocaleDateString('zh-CN', {
                                            month: 'numeric',
                                            day: 'numeric',
                                        })}
                                    </time>
                                </div>
                            </button>
                        ))}
                    </nav>
                </aside>
                <section className="flex min-h-0 min-w-0 flex-1 flex-col">
                    {info && (
                        <header className="flex shrink-0 items-center justify-between gap-3 border-b border-border-subtle px-4 py-3 sm:px-6">
                            <div className="min-w-0 space-y-1">
                                <div className="flex flex-wrap items-center gap-2">
                                    <h2 className="truncate text-sm font-semibold text-text">
                                        {info.botName}
                                    </h2>
                                    <Badge
                                        tone={
                                            info.state === 'failed'
                                                ? 'danger'
                                                : info.state === 'waitingForInput'
                                                  ? 'warning'
                                                  : isAssistantBusy(info.state)
                                                    ? 'brand'
                                                    : 'neutral'
                                        }
                                    >
                                        {assistantStateLabel[info.state]}
                                    </Badge>
                                </div>
                                <p className="truncate text-2xs text-text-tertiary">
                                    {providerLabel(info.provider)} · {info.botId}
                                </p>
                            </div>
                            <div className="flex shrink-0 items-center gap-1">
                                {info.state !== 'closed' && (
                                    <Button
                                        size="sm"
                                        variant="ghost"
                                        disabled={assistant.pending.includes(`close:${info.id}`)}
                                        onClick={() => {
                                            void assistant.actions.close(info.id);
                                        }}
                                    >
                                        关闭会话
                                    </Button>
                                )}
                                <Button
                                    size="icon"
                                    variant="ghost"
                                    className="h-7 w-7"
                                    aria-label="删除此会话"
                                    onClick={() => setDeleteId(info.id)}
                                >
                                    <Trash2 size={14} />
                                </Button>
                            </div>
                        </header>
                    )}
                    {info?.error && (
                        <p
                            role="alert"
                            className="shrink-0 whitespace-pre-wrap break-words border-b border-border-subtle bg-danger-soft/50 px-4 py-2 text-xs leading-5 text-danger"
                        >
                            {info.error}
                        </p>
                    )}
                    {info && <MiniMaxLogin key={`login:${info.id}`} info={info} />}
                    {!assistant.selectedId ? (
                        <NewSession
                            bots={bots}
                            botId={targetBot}
                            provider={provider}
                            settings={preferences.settings.data}
                            providers={preferences.providers.data ?? []}
                            modelStatus={preferences.modelStatus.data}
                            modelError={preferences.modelStatus.error?.message ?? null}
                            loading={
                                preferences.settings.isPending ||
                                botQuery.isPending ||
                                ((provider ??
                                    preferences.settings.data?.defaultProvider ??
                                    'builtin') === 'builtin' &&
                                    preferences.modelStatus.isPending)
                            }
                            providersLoading={preferences.providers.isPending}
                            externalOpen={externalOpen}
                            onExternalOpen={setExternalOpen}
                            creating={assistant.pending.includes('create')}
                            onBot={setTargetBot}
                            onProvider={setProvider}
                            onCreate={(chosen, botId) => {
                                void assistant.actions.create(chosen, botId);
                            }}
                            onSettings={() => setSettingsOpen(true)}
                            onBots={() => onNavigate('bots')}
                        />
                    ) : session ? (
                        <Conversation
                            key={`conversation:${session.info.id}`}
                            session={session}
                            drafts={assistant.drafts}
                            pending={assistant.pending}
                            onNew={newSession}
                        />
                    ) : (
                        <div
                            role="status"
                            className="m-auto flex items-center gap-2 p-6 text-sm text-text-secondary"
                        >
                            <Loader2 size={16} className="animate-spin" />
                            读取会话记录…
                        </div>
                    )}
                </section>
            </div>
            <Dialog open={settingsOpen} onOpenChange={setSettingsOpen}>
                <DialogContent size="sheet">
                    <DialogHeader>
                        <DialogTitle>助手设置</DialogTitle>
                    </DialogHeader>
                    <div className="min-h-0 overflow-y-auto py-4 pr-1">
                        <AgentSection />
                    </div>
                </DialogContent>
            </Dialog>
            <Dialog
                open={deleteId !== null}
                onOpenChange={(open) => {
                    if (!open) setDeleteId(null);
                }}
            >
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>删除会话记录？</DialogTitle>
                        <DialogDescription>
                            会话将停止并从列表移除。Agent 工作目录、登录信息及备份会保留。
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button onClick={() => setDeleteId(null)}>取消</Button>
                        <Button
                            variant="danger"
                            disabled={
                                !!deleteId && assistant.pending.includes(`delete:${deleteId}`)
                            }
                            onClick={() => {
                                if (deleteId)
                                    void assistant.actions.delete(deleteId).then((ok) => {
                                        if (ok) setDeleteId(null);
                                    });
                            }}
                        >
                            删除会话
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </div>
    );
}
