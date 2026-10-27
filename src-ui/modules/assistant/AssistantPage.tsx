// 诊断助手工作区：会话列表、目标与对话。

import { useEffect, useMemo, useRef, useState } from 'react';
import { Loader2, MessageSquare, MoreHorizontal, Plus, Power, Trash2 } from 'lucide-react';
import type { AssistantProvider } from '../../core/domain/assistant/types';
import type { AppRoute } from '../../core/domain/ui/route';
import { assistantStateLabel } from '../../core/domain/assistant/session';
import { useAssistant, useAssistantSettings } from '../../hooks/assistant/useAssistant';
import { consumeAssistantTarget, useAssistantTarget } from '../../hooks/assistant/assistantNav';
import { NEW_SESSION_DRAFT } from '../../hooks/assistant/assistantStore';
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
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from '../../shared/ui';
import { Conversation } from './Conversation';
import { NewSession } from './NewSession';
import { AgentSection } from './settings/AgentSection';
import { MiniMaxLogin } from './MiniMaxLogin';
import { SessionSidebar } from './SessionSidebar';
import { sessionStateTone } from './sessionPresentation';

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
    const openSessionsRef = useRef<HTMLButtonElement>(null);
    const closeSessionsRef = useRef<HTMLButtonElement>(null);
    const sessionsWereOpen = useRef(false);
    const [deleteId, setDeleteId] = useState<string | null>(null);
    const [discover, setDiscover] = useState(false);
    const info = assistant.infos.find((item) => item.id === assistant.selectedId);
    const session = assistant.selectedId ? assistant.sessions[assistant.selectedId] : undefined;
    const discoverProviders =
        discover ||
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
            setDiscover(true);
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
        preferences.modelStatus.error ??
        (discoverProviders ? preferences.providers.error : null) ??
        botQuery.error;
    const error =
        assistant.error ??
        (queryError
            ? queryError instanceof Error
                ? queryError.message
                : String(queryError)
            : null);
    const sessionsButton = (
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
    );
    return (
        <div
            className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden"
            style={{ marginBottom: 'var(--terminal-dock-inset, 0px)' }}
        >
            <div className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden">
                <SessionSidebar
                    infos={assistant.infos}
                    selectedId={assistant.selectedId}
                    loading={assistant.loading}
                    preview={assistant.preview}
                    open={showSessions}
                    closeButtonRef={closeSessionsRef}
                    onClose={() => setShowSessions(false)}
                    onRefresh={() => {
                        void assistant.actions.refresh();
                        void preferences.modelStatus.refetch();
                        if (discoverProviders) void preferences.providers.refetch();
                    }}
                    onSettings={() => setSettingsOpen(true)}
                    onNew={newSession}
                    onSelect={(id) => {
                        assistant.actions.select(id);
                        setShowSessions(false);
                    }}
                />
                <section
                    className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-tl-md border-l border-t border-border-subtle bg-surface"
                    {...(showSessions ? { inert: '' } : {})}
                >
                    {!info && (
                        <div className="flex h-11 shrink-0 items-center gap-2 border-b border-border-subtle px-3 md:hidden">
                            {sessionsButton}
                            <span className="text-sm font-medium text-text">诊断助手</span>
                        </div>
                    )}
                    {assistant.preview && (
                        <p className="border-b border-border-subtle px-4 py-2 text-2xs text-text-tertiary md:hidden">
                            浏览器预览，不会调用真实模型或操作 Bot。
                        </p>
                    )}
                    {info && (
                        <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border-subtle px-4 sm:px-6">
                            {sessionsButton}
                            <div className="min-w-0 flex-1">
                                <div className="flex min-w-0 items-center gap-2">
                                    <h2 className="truncate text-sm font-semibold text-text">
                                        {info.botName}
                                    </h2>
                                    <Badge tone={sessionStateTone[info.state]} dot>
                                        {assistantStateLabel[info.state]}
                                    </Badge>
                                </div>
                                <p className="mt-0.5 truncate text-2xs text-text-tertiary">
                                    {providerLabel(info.provider)} · {info.botId}
                                </p>
                            </div>
                            <DropdownMenu>
                                <DropdownMenuTrigger asChild>
                                    <Button
                                        size="icon"
                                        variant="ghost"
                                        className="h-8 w-8 shrink-0"
                                        aria-label="会话操作"
                                    >
                                        <MoreHorizontal size={16} />
                                    </Button>
                                </DropdownMenuTrigger>
                                <DropdownMenuContent align="end" className="min-w-[160px]">
                                    <DropdownMenuItem onSelect={newSession}>
                                        <Plus size={13} />
                                        以此 Bot 新建会话
                                    </DropdownMenuItem>
                                    {info.state !== 'closed' && (
                                        <DropdownMenuItem
                                            disabled={assistant.pending.includes(
                                                `close:${info.id}`,
                                            )}
                                            onSelect={() => {
                                                void assistant.actions.close(info.id);
                                            }}
                                        >
                                            <Power size={13} />
                                            关闭会话
                                        </DropdownMenuItem>
                                    )}
                                    <DropdownMenuSeparator />
                                    <DropdownMenuItem
                                        tone="danger"
                                        onSelect={() => setDeleteId(info.id)}
                                    >
                                        <Trash2 size={13} />
                                        删除会话
                                    </DropdownMenuItem>
                                </DropdownMenuContent>
                            </DropdownMenu>
                        </header>
                    )}
                    {(error || assistant.notices.length > 0) && (
                        <div className="shrink-0 space-y-2 border-b border-warning/20 bg-warning-soft/50 px-4 py-2 text-xs leading-5 sm:px-6">
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
                                        重新加载
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
                    {info?.error && (
                        <p
                            role="alert"
                            className="shrink-0 whitespace-pre-wrap break-words border-b border-border-subtle bg-danger-soft/50 px-4 py-2 text-xs leading-5 text-danger sm:px-6"
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
                            loading={
                                preferences.settings.isPending ||
                                botQuery.isPending ||
                                ((provider ??
                                    preferences.settings.data?.defaultProvider ??
                                    'builtin') === 'builtin' &&
                                    preferences.modelStatus.isPending)
                            }
                            providersLoading={discoverProviders && preferences.providers.isPending}
                            creating={assistant.pending.includes('create')}
                            draft={assistant.drafts[NEW_SESSION_DRAFT] ?? ''}
                            onDraft={(value) => assistant.actions.draft(NEW_SESSION_DRAFT, value)}
                            onBot={setTargetBot}
                            onProvider={setProvider}
                            onDiscover={() => setDiscover(true)}
                            onStart={(chosen, botId, text) => {
                                void assistant.actions.start(chosen, botId, text);
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
                    <div className="min-h-0 overflow-y-auto pr-1 pt-2">
                        <AgentSection embedded />
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
