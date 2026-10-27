// 诊断助手工作区：会话列表、目标与对话。

import { useEffect, useMemo, useRef, useState } from 'react';
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
    const sessionsRef = useRef<HTMLElement>(null);
    const openSessionsRef = useRef<HTMLButtonElement>(null);
    const closeSessionsRef = useRef<HTMLButtonElement>(null);
    const sessionsWereOpen = useRef(false);
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
        if (showSessions) {
            sessionsWereOpen.current = true;
            closeSessionsRef.current?.focus();
        } else if (sessionsWereOpen.current) {
            sessionsWereOpen.current = false;
            openSessionsRef.current?.focus();
        }
    }, [showSessions]);
    useEffect(() => {
        if (!window.matchMedia) return;
        const desktop = window.matchMedia('(min-width: 768px)');
        if (!desktop?.addEventListener) return;
        const onChange = () => {
            if (desktop.matches) setShowSessions(false);
        };
        desktop.addEventListener('change', onChange);
        return () => desktop.removeEventListener('change', onChange);
    }, []);
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
        <div
            className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden"
            style={{ marginBottom: 'var(--terminal-dock-inset, 0px)' }}
        >
            <div className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden">
                <aside
                    ref={sessionsRef}
                    id="assistant-sessions"
                    aria-label="诊断会话列表"
                    onKeyDown={(event) => {
                        if (!showSessions) return;
                        if (event.key === 'Escape') {
                            event.preventDefault();
                            setShowSessions(false);
                        } else if (event.key === 'Tab') {
                            const buttons = Array.from(
                                sessionsRef.current?.querySelectorAll<HTMLButtonElement>(
                                    'button:not(:disabled)',
                                ) ?? [],
                            ).filter((button) => button.getClientRects().length > 0);
                            const first = buttons[0];
                            const last = buttons[buttons.length - 1];
                            if (event.shiftKey && document.activeElement === first && last) {
                                event.preventDefault();
                                last.focus();
                            } else if (
                                !event.shiftKey &&
                                document.activeElement === last &&
                                first
                            ) {
                                event.preventDefault();
                                first.focus();
                            }
                        }
                    }}
                    className={cn(
                        'absolute inset-0 z-20 flex-col bg-canvas px-3 pb-3 pt-1 md:static md:z-auto md:flex md:w-[260px] md:shrink-0 md:bg-transparent',
                        showSessions ? 'flex' : 'hidden',
                    )}
                >
                    <div className="flex h-10 shrink-0 items-center gap-1 px-1">
                        <h1 className="min-w-0 flex-1 truncate text-sm font-semibold text-text">
                            诊断助手
                        </h1>
                        <Button
                            size="icon"
                            variant="ghost"
                            className="h-7 w-7"
                            aria-label="刷新诊断会话"
                            disabled={assistant.loading}
                            onClick={() => {
                                void assistant.actions.refresh();
                                void preferences.modelStatus.refetch();
                                if (discoverProviders) void preferences.providers.refetch();
                            }}
                        >
                            <RefreshCw
                                size={15}
                                className={assistant.loading ? 'animate-spin' : undefined}
                            />
                        </Button>
                        <Button
                            size="icon"
                            variant="ghost"
                            className="h-7 w-7"
                            aria-label="助手设置"
                            onClick={() => setSettingsOpen(true)}
                        >
                            <Settings2 size={15} />
                        </Button>
                    </div>
                    <div className="flex shrink-0 items-center gap-2 pb-3 pt-1">
                        <Button size="sm" className="flex-1" onClick={newSession}>
                            <Plus size={15} />
                            新建会话
                        </Button>
                        <Button
                            ref={closeSessionsRef}
                            size="icon"
                            variant="ghost"
                            className="h-7 w-7 md:hidden"
                            aria-label="收起会话列表"
                            onClick={() => setShowSessions(false)}
                        >
                            <X size={15} />
                        </Button>
                    </div>
                    <div className="flex shrink-0 items-center justify-between px-5 pb-2 pt-1 text-2xs font-medium text-text-tertiary">
                        <span>诊断记录</span>
                        {assistant.infos.length > 0 && <span>{assistant.infos.length}</span>}
                    </div>
                    <nav
                        className="min-h-0 flex-1 space-y-1 overflow-y-auto px-2.5 pb-3"
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
                                        ? 'bg-inset'
                                        : 'hover:bg-inset/60',
                                )}
                            >
                                <div className="flex min-w-0 items-center gap-2">
                                    <span className="min-w-0 flex-1 truncate text-sm font-medium text-text">
                                        {item.title || '新诊断'}
                                    </span>
                                    <time
                                        className="shrink-0 text-2xs text-text-tertiary"
                                        dateTime={new Date(item.updatedAtMs).toISOString()}
                                    >
                                        {new Date(item.updatedAtMs).toLocaleDateString('zh-CN', {
                                            month: 'numeric',
                                            day: 'numeric',
                                        })}
                                    </time>
                                </div>
                                <div className="flex min-w-0 items-center gap-2 text-2xs text-text-tertiary">
                                    <span className="min-w-0 flex-1 truncate">
                                        {item.botName} · {providerLabel(item.provider)}
                                    </span>
                                    <span className="shrink-0">
                                        {assistantStateLabel[item.state]}
                                    </span>
                                </div>
                            </button>
                        ))}
                    </nav>
                    {assistant.preview && (
                        <p className="border-t border-border-subtle px-1 pt-3 text-2xs leading-5 text-text-tertiary">
                            当前为浏览器界面演示，不会调用真实模型或操作 Bot。
                        </p>
                    )}
                </aside>
                <section
                    className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-tl-md border-l border-t border-border-subtle bg-surface"
                    {...(showSessions ? { inert: '' } : {})}
                >
                    {!info && (
                        <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border-subtle px-3 md:hidden">
                            <Button
                                ref={openSessionsRef}
                                size="icon"
                                variant="ghost"
                                className="h-8 w-8"
                                aria-label="显示诊断会话列表"
                                aria-expanded={showSessions}
                                aria-controls="assistant-sessions"
                                onClick={() => setShowSessions(true)}
                            >
                                <MessageSquare size={17} />
                            </Button>
                            <span className="text-sm font-medium text-text">诊断助手</span>
                        </div>
                    )}
                    {assistant.preview && (
                        <p className="border-b border-border-subtle px-4 py-2 text-2xs text-text-tertiary md:hidden">
                            浏览器预览，不会调用真实模型或操作 Bot。
                        </p>
                    )}
                    {(error || assistant.notices.length > 0) && (
                        <div className="shrink-0 space-y-2 border-b border-warning/20 bg-warning-soft/50 px-4 py-2 text-xs leading-5">
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
                                            if (discoverProviders)
                                                void preferences.providers.refetch();
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
                    {info && (
                        <header className="flex shrink-0 items-center justify-between gap-3 border-b border-border-subtle px-4 py-3.5 sm:px-6">
                            <Button
                                ref={openSessionsRef}
                                size="icon"
                                variant="ghost"
                                className="h-8 w-8 shrink-0 md:hidden"
                                aria-label="显示诊断会话列表"
                                aria-expanded={showSessions}
                                aria-controls="assistant-sessions"
                                onClick={() => setShowSessions(true)}
                            >
                                <MessageSquare size={17} />
                            </Button>
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
