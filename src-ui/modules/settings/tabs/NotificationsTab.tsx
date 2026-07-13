// 通知设置：掉线时如何通知你。
// 主列表只留开关 + 通道摘要；通道详情 / 消息模板收敛进 Dialog（对齐连接配置交互）。
// 颜色只走语义 token，随主题切换。

import { useEffect, useRef, useState } from 'react';
import { History, Pencil, Plus, Trash2, X } from 'lucide-react';
import { DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED } from '../../../core/domain/ui/infoBarDismiss';
import {
    createBlankWebhookChannel,
    DEFAULT_ONEBOT_MESSAGE,
    type WebhookChannelDraft,
} from '../../../core/domain/settings/offline-notify-defaults';
import { settingsService } from '../../../core/services/settings.service';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import {
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    NumberField,
    Select,
    Switch,
} from '../../../shared/ui';
import { ActionMotionIcon } from '../../../shared/ui/motion';
import { cn } from '../../../shared/utils/cn';
import type { SettingsDraft } from '../settings-draft';
import {
    FieldRow,
    InfoBarDismissDurationSlider,
    InfoBarDismissSliderPresence,
    SettingsSection,
    SettingsTabSections,
} from '../_shared';
import { NcdWatchRemoteSection } from './NcdWatchRemoteSection';
import {
    EmailEditorDialog,
    emailIsReady,
    emailSummary,
    type EmailEditorDraft,
} from './notifications/EmailEditorDialog';
import { OneBotMessageEditor } from './notifications/OneBotMessageEditor';
import {
    OneBotMessengerPicker,
    type OneBotCandidate,
} from './notifications/OneBotMessengerPicker';
import {
    WebhookChannelEditorDialog,
    type ChannelEditorState,
} from './notifications/WebhookChannelEditorDialog';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
    /** 草稿未保存时禁用 ncd-watch 同步（避免写旧 Webhook） */
    settingsDirty?: boolean;
}

const ONEBOT_TARGET_ITEMS = [
    { value: 'private', label: '私聊' },
    { value: 'group', label: '群聊' },
] as const;

type OneBotEditorDraft = Pick<
    SettingsDraft,
    | 'onebotMessengerBotIds'
    | 'onebotTargetType'
    | 'onebotTargetIds'
    | 'onebotMessageTemplate'
>;

function newChannelId(existing: WebhookChannelDraft[]): string {
    const stamp = Date.now().toString(36);
    let n = existing.length + 1;
    let id = `channel-${n}-${stamp}`;
    while (existing.some((c) => c.id === id)) {
        n += 1;
        id = `channel-${n}-${stamp}`;
    }
    return id;
}

function channelDisplayName(ch: WebhookChannelDraft): string {
    const n = ch.name.trim();
    if (n) return n;
    if (ch.url.trim()) {
        try {
            return new URL(ch.url).hostname || ch.id;
        } catch {
            return ch.url.trim().slice(0, 28) || ch.id;
        }
    }
    return '未命名通道';
}

function channelSummary(ch: WebhookChannelDraft): string {
    if (ch.url.trim()) {
        try {
            return new URL(ch.url).host;
        } catch {
            return ch.url.trim();
        }
    }
    return '未填写地址';
}

function oneBotIsReady(oneBot: OneBotEditorDraft): boolean {
    return (
        oneBot.onebotMessengerBotIds.some((id) => id.trim()) &&
        oneBot.onebotTargetIds.some((id) => id > 0)
    );
}

function oneBotSummary(oneBot: OneBotEditorDraft): string {
    const messengers = oneBot.onebotMessengerBotIds.filter((id) => id.trim());
    const targets = oneBot.onebotTargetIds.filter((id) => id > 0);
    if (messengers.length === 0) return '尚未选择发送方 Bot';
    if (targets.length === 0) return '待填写接收目标';
    const messengerLabel =
        messengers.length === 1
            ? messengers[0]
            : `${messengers[0]} 等 ${messengers.length} 个`;
    const targetLabel =
        targets.length === 1
            ? String(targets[0])
            : `${targets[0]} 等 ${targets.length} 个`;
    return `${messengerLabel} → ${oneBot.onebotTargetType === 'group' ? '群' : '私聊'} ${targetLabel}`;
}

function parseTargetTokens(raw: string): number[] {
    return raw
        .split(/[,，;；\s]+/)
        .map((part) => Number(part.trim()))
        .filter((n) => Number.isFinite(n) && n > 0)
        .map((n) => Math.round(n));
}

function historyKindLabel(kind: string): string {
    switch (kind) {
        case 'auto_restart':
        case 'manual':
            return '掉线';
        case 'kicked':
            return '被踢下线';
        case 'process_crashed':
            return '异常退出';
        case 'recovered':
            return '恢复上线';
        default:
            return kind;
    }
}

function DeliveryResultBadge({
    label,
    value,
}: {
    label: string;
    value: 'ok' | 'failed' | 'skipped';
}) {
    if (value === 'ok') {
        return <Badge tone="success" appearance="soft">{label} 已发送</Badge>;
    }
    if (value === 'failed') {
        return <Badge tone="danger" appearance="soft">{label} 失败</Badge>;
    }
    return <Badge tone="neutral" appearance="soft">{label} 未投递</Badge>;
}

function Chip({
    label,
    onRemove,
    tone = 'neutral',
}: {
    label: string;
    onRemove: () => void;
    tone?: 'neutral' | 'success' | 'warning';
}) {
    return (
        <span
            className={cn(
                'inline-flex max-w-full items-center gap-1 rounded-sm border px-2 py-1 text-[12px]',
                tone === 'success' &&
                    'border-success/30 bg-success-soft text-text',
                tone === 'warning' &&
                    'border-warning/30 bg-warning-soft text-text',
                tone === 'neutral' &&
                    'border-border-subtle bg-inset text-text-secondary',
            )}
        >
            <span className="truncate">{label}</span>
            <button
                type="button"
                aria-label={`移除 ${label}`}
                onClick={onRemove}
                className="rounded-xs p-0.5 text-text-tertiary transition-colors hover:bg-field hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
            >
                <X size={12} />
            </button>
        </span>
    );
}

function TargetIdChipsInput({
    values,
    onChange,
    placeholder,
    className,
}: {
    values: number[];
    onChange: (next: number[]) => void;
    placeholder?: string;
    className?: string;
}) {
    const [draft, setDraft] = useState('');
    const inputRef = useRef<HTMLInputElement | null>(null);

    const commitDraft = (raw: string) => {
        const tokens = parseTargetTokens(raw);
        if (tokens.length === 0) {
            setDraft('');
            return;
        }
        const next = [...values];
        for (const token of tokens) {
            if (!next.includes(token)) next.push(token);
        }
        onChange(next);
        setDraft('');
    };

    return (
        <div
            className={cn(
                'flex min-h-10 w-full content-start items-start gap-1.5 overflow-y-auto rounded-sm border border-border-subtle bg-field px-2 py-1.5',
                'flex-wrap focus-within:border-brand focus-within:ring-2 focus-within:ring-brand focus-within:ring-inset',
                className,
            )}
            onClick={() => inputRef.current?.focus()}
        >
            {values.map((id) => (
                <Chip
                    key={id}
                    label={String(id)}
                    onRemove={() => onChange(values.filter((item) => item !== id))}
                />
            ))}
            <input
                ref={inputRef}
                value={draft}
                placeholder={values.length === 0 ? placeholder : '继续输入，逗号确认'}
                onChange={(event) => {
                    const next = event.target.value;
                    if (/[,，;；\n]/.test(next)) {
                        commitDraft(next);
                        return;
                    }
                    setDraft(next);
                }}
                onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ',' || event.key === '，') {
                        event.preventDefault();
                        commitDraft(draft);
                        return;
                    }
                    if (event.key === 'Backspace' && !draft && values.length > 0) {
                        onChange(values.slice(0, -1));
                    }
                }}
                onBlur={() => {
                    if (draft.trim()) commitDraft(draft);
                }}
                className="min-w-[9rem] flex-1 bg-transparent py-0.5 text-sm text-text outline-none placeholder:text-text-tertiary"
            />
        </div>
    );
}

export function NotificationsTab({
    draft,
    patchDraft,
    settingsDirty = false,
}: Props) {
    const [testing, setTesting] = useState<string | null>(null);
    const [editor, setEditor] = useState<ChannelEditorState | null>(null);
    const [editorMount, setEditorMount] = useState<ChannelEditorState | null>(
        null,
    );
    const [deleteId, setDeleteId] = useState<string | null>(null);
    const [emailEditorOpen, setEmailEditorOpen] = useState(false);
    const [emailEditorDraft, setEmailEditorDraft] =
        useState<EmailEditorDraft | null>(null);
    const [emailPreset, setEmailPreset] = useState('');
    const [oneBotEditorOpen, setOneBotEditorOpen] = useState(false);
    const [oneBotEditorDraft, setOneBotEditorDraft] =
        useState<OneBotEditorDraft | null>(null);
    const [oneBotCandidates, setOneBotCandidates] = useState<OneBotCandidate[]>(
        [],
    );
    const [oneBotCandidatesLoading, setOneBotCandidatesLoading] =
        useState(false);
    const [oneBotEnablingId, setOneBotEnablingId] = useState<string | null>(
        null,
    );
    const [history, setHistory] = useState<
        Awaited<ReturnType<typeof settingsService.listOfflineDeliveryHistory>>
    >([]);
    const [historyLoading, setHistoryLoading] = useState(false);
    const [historyDialogOpen, setHistoryDialogOpen] = useState(false);

    useEffect(() => {
        if (editor !== null) setEditorMount(editor);
    }, [editor]);

    const refreshHistory = async () => {
        setHistoryLoading(true);
        try {
            setHistory(await settingsService.listOfflineDeliveryHistory());
        } catch {
            setHistory([]);
        } finally {
            setHistoryLoading(false);
        }
    };

    useEffect(() => {
        void refreshHistory();
    }, []);

    const openHistoryDialog = () => {
        setHistoryDialogOpen(true);
        void refreshHistory();
    };

    if (!draft) {
        return (
            <p className="text-[13px] text-text-tertiary">正在加载设置…</p>
        );
    }

    const channels = draft.webHookChannels;
    const deleteTarget = channels.find((c) => c.id === deleteId) ?? null;
    const enabledChannelCount = channels.filter(
        (c) => c.enabled && c.url.trim(),
    ).length;

    const updateChannels = (next: WebhookChannelDraft[]) => {
        const first = next.find((c) => c.enabled && c.url.trim()) ?? next[0];
        patchDraft({
            webHookChannels: next,
            webHookUrl: first?.url ?? '',
            webHookSecret: first?.secret ?? '',
            webHookJson: first?.bodyTemplate ?? draft.webHookJson,
            webHookMethod: first?.method ?? 'POST',
        });
    };

    const openCreate = () => {
        const id = newChannelId(channels);
        setEditor({
            mode: 'create',
            draft: createBlankWebhookChannel(id, `通道 ${channels.length + 1}`),
        });
    };

    const openEdit = (ch: WebhookChannelDraft) => {
        setEditor({ mode: 'edit', id: ch.id, draft: { ...ch } });
    };

    const closeEditor = () => setEditor(null);

    const patchEditorDraft = (patch: Partial<WebhookChannelDraft>) => {
        setEditor((cur) =>
            cur ? { ...cur, draft: { ...cur.draft, ...patch } } : cur,
        );
    };

    const saveEditor = () => {
        if (!editor) return;
        const nextDraft: WebhookChannelDraft = {
            ...editor.draft,
            name: editor.draft.name.trim(),
            url: editor.draft.url.trim(),
            method: editor.draft.method === 'GET' ? 'GET' : 'POST',
            bodyTemplate: editor.draft.bodyTemplate.trim()
                ? editor.draft.bodyTemplate
                : createBlankWebhookChannel(editor.draft.id).bodyTemplate,
        };
        if (editor.mode === 'create') {
            updateChannels([...channels, nextDraft]);
        } else {
            updateChannels(
                channels.map((c) => (c.id === editor.id ? nextDraft : c)),
            );
        }
        setEditor(null);
    };

    const confirmDelete = () => {
        if (!deleteId) return;
        updateChannels(channels.filter((c) => c.id !== deleteId));
        setDeleteId(null);
    };

    const openEmailEditor = () => {
        setEmailPreset('');
        setEmailEditorDraft({
            emailSender: draft.emailSender,
            emailReceiver: draft.emailReceiver,
            emailToken: draft.emailToken,
            emailSmtpServer: draft.emailSmtpServer,
            emailSmtpPort: draft.emailSmtpPort || 465,
            emailEncryption: draft.emailEncryption || 'SSL',
        });
        setEmailEditorOpen(true);
    };

    const saveEmailEditor = () => {
        if (!emailEditorDraft) return;
        patchDraft({
            emailSender: emailEditorDraft.emailSender,
            emailReceiver: emailEditorDraft.emailReceiver,
            emailToken: emailEditorDraft.emailToken,
            emailSmtpServer: emailEditorDraft.emailSmtpServer,
            emailSmtpPort: emailEditorDraft.emailSmtpPort,
            emailEncryption: emailEditorDraft.emailEncryption,
        });
        setEmailEditorOpen(false);
    };

    const openOneBotEditor = () => {
        const messengerIds =
            draft.onebotMessengerBotIds.length > 0
                ? [...draft.onebotMessengerBotIds]
                : draft.onebotMessengerBotId.trim()
                  ? [draft.onebotMessengerBotId.trim()]
                  : [];
        const targetIds =
            draft.onebotTargetIds.length > 0
                ? [...draft.onebotTargetIds]
                : draft.onebotTargetId > 0
                  ? [draft.onebotTargetId]
                  : [];
        setOneBotEditorDraft({
            onebotMessengerBotIds: messengerIds,
            onebotTargetType:
                draft.onebotTargetType === 'group' ? 'group' : 'private',
            onebotTargetIds: targetIds,
            onebotMessageTemplate:
                draft.onebotMessageTemplate || DEFAULT_ONEBOT_MESSAGE,
        });
        setOneBotEditorOpen(true);
        setOneBotCandidatesLoading(true);
        void settingsService
            .listOneBotMessengerCandidates()
            .then((items) => setOneBotCandidates(items))
            .catch(() => setOneBotCandidates([]))
            .finally(() => setOneBotCandidatesLoading(false));
    };

    const saveOneBotEditor = () => {
        if (!oneBotEditorDraft) return;
        const messengerIds = oneBotEditorDraft.onebotMessengerBotIds
            .map((id) => id.trim())
            .filter(Boolean);
        const targetIds = oneBotEditorDraft.onebotTargetIds.filter((id) => id > 0);
        patchDraft({
            onebotMessengerBotIds: messengerIds,
            onebotMessengerBotId: messengerIds[0] ?? '',
            onebotTargetType: oneBotEditorDraft.onebotTargetType,
            onebotTargetIds: targetIds,
            onebotTargetId: targetIds[0] ?? 0,
            onebotMessageTemplate:
                oneBotEditorDraft.onebotMessageTemplate.trim() ||
                DEFAULT_ONEBOT_MESSAGE,
        });
        setOneBotEditorOpen(false);
    };

    const ensureOneBotHttp = async (botId: string) => {
        setOneBotEnablingId(botId);
        try {
            const result = await settingsService.ensureOneBotMessengerHttp(botId);
            setOneBotCandidates((current) => {
                const next = current.filter((item) => item.bot_id !== botId);
                next.push(result.candidate);
                return next;
            });
            setOneBotEditorDraft((current) => {
                if (!current) return current;
                if (current.onebotMessengerBotIds.includes(botId)) return current;
                return {
                    ...current,
                    onebotMessengerBotIds: [...current.onebotMessengerBotIds, botId],
                };
            });
            const actionText =
                result.action === 'already_ready'
                    ? '已具备环回 HTTP'
                    : result.action === 'enabled'
                      ? '已启用现有 HTTP 服务'
                      : '已自动创建环回 HTTP 服务';
            const scopeHint =
                result.candidate.scope === 'remote'
                    ? '远端配置已写入；保存后请同步 ncd-watch。运行中时会尽量热更新。'
                    : '若 Bot 正在运行，会热更新连接配置。';
            pushInfoBar({
                key: 'onebot-enable-http',
                tone: 'success',
                title: actionText,
                content: `${result.candidate.name || botId} · 端口 ${result.port}。${scopeHint}`,
            });
        } catch (err) {
            pushInfoBar({
                key: 'onebot-enable-http',
                tone: 'danger',
                title: '自动配置 HTTP 失败',
                content: err instanceof Error ? err.message : String(err),
            });
        } finally {
            setOneBotEnablingId(null);
        }
    };

    const runWebhookTest = async (channelId: string) => {
        setTesting(`webhook:${channelId}`);
        try {
            await settingsService.testWebhook(channelId);
            pushInfoBar({
                key: 'offline-webhook-test',
                tone: 'success',
                title: '测试已发送',
                content:
                    '请到目标服务确认是否收到。若刚改过配置，请先保存设置。',
            });
        } catch (err) {
            pushInfoBar({
                key: 'offline-webhook-test',
                tone: 'danger',
                title: '测试失败',
                content: err instanceof Error ? err.message : String(err),
            });
        } finally {
            setTesting(null);
        }
    };

    const editorWorking = editor ?? editorMount;
    const emailDraft = emailEditorDraft ?? {
        emailSender: draft.emailSender,
        emailReceiver: draft.emailReceiver,
        emailToken: draft.emailToken,
        emailSmtpServer: draft.emailSmtpServer,
        emailSmtpPort: draft.emailSmtpPort || 465,
        emailEncryption: draft.emailEncryption || 'SSL',
    };
    const oneBotDraft = oneBotEditorDraft ?? {
        onebotMessengerBotIds:
            draft.onebotMessengerBotIds.length > 0
                ? draft.onebotMessengerBotIds
                : draft.onebotMessengerBotId.trim()
                  ? [draft.onebotMessengerBotId.trim()]
                  : [],
        onebotTargetType:
            draft.onebotTargetType === 'group' ? 'group' : 'private',
        onebotTargetIds:
            draft.onebotTargetIds.length > 0
                ? draft.onebotTargetIds
                : draft.onebotTargetId > 0
                  ? [draft.onebotTargetId]
                  : [],
        onebotMessageTemplate:
            draft.onebotMessageTemplate || DEFAULT_ONEBOT_MESSAGE,
    };

    return (
        <SettingsTabSections>
            <SettingsSection
                title="桌面通知"
                description="主窗口隐藏或进入轻量模式时，仍可通过系统通知提醒你"
            >
                <FieldRow
                    label="Bot 掉线"
                    description="还需在对应 Bot 高级设置里打开「掉线时发送通知」（NapCat / SnowLuma 均需；此开关同时门控 Webhook / 邮件 / OneBot）"
                >
                    <Switch
                        checked={draft.notifyOnOffline}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnOffline: v })
                        }
                    />
                </FieldRow>
                <FieldRow
                    label="Bot 异常退出"
                    description="进程非正常结束时（全局）"
                >
                    <Switch
                        checked={draft.notifyOnBotCrashed}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnBotCrashed: v })
                        }
                    />
                </FieldRow>
                <FieldRow
                    label="被踢下线"
                    description="QQ 被踢或登录失效时（全局）"
                    isLast
                >
                    <Switch
                        checked={draft.notifyOnLoginKicked}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnLoginKicked: v })
                        }
                    />
                </FieldRow>
            </SettingsSection>

            <NcdWatchRemoteSection settingsDirty={settingsDirty} />

            <SettingsSection
                title="投递行为"
                description="恢复通知、重复掉线防抖与内存投递历史（不落盘，重启清空）"
            >
                <FieldRow
                    label="上线恢复通知"
                    description="掉线后又上线时再推一次（默认关；Webhook/邮件/OneBot 同样走总开关）"
                >
                    <Switch
                        checked={draft.notifyOnRecovered}
                        onCheckedChange={(v) =>
                            patchDraft({ notifyOnRecovered: v })
                        }
                    />
                </FieldRow>
                <FieldRow
                    label="掉线防抖（秒）"
                    description="同一 Bot 在窗口内重复 offline 边沿只投递一次；0 关闭"
                >
                    <NumberField
                        value={draft.offlineDebounceSeconds}
                        min={0}
                        max={600}
                        step={1}
                        onValueChange={(v) =>
                            patchDraft({
                                offlineDebounceSeconds: Math.max(
                                    0,
                                    Math.min(600, Math.round(v || 0)),
                                ),
                            })
                        }
                        className="w-28"
                    />
                </FieldRow>
                <FieldRow
                    label="历史条数上限"
                    description="内存保留最近 N 条投递记录；0 不记录"
                >
                    <NumberField
                        value={draft.offlineDeliveryHistoryLimit}
                        min={0}
                        max={200}
                        step={1}
                        onValueChange={(v) =>
                            patchDraft({
                                offlineDeliveryHistoryLimit: Math.max(
                                    0,
                                    Math.min(200, Math.round(v || 0)),
                                ),
                            })
                        }
                        className="w-28"
                    />
                </FieldRow>
                <FieldRow
                    label="投递记录"
                    description={
                        history.length > 0
                            ? `本次运行已有 ${history.length} 条记录；重启后自动清空`
                            : '查看本次运行中各渠道的投递结果；重启后自动清空'
                    }
                    isLast
                >
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        onClick={openHistoryDialog}
                    >
                        <ActionMotionIcon icon={History} size={14} />
                        查看记录
                    </Button>
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title="Webhook 推送"
                description="掉线时向配置的地址发 JSON。启用后可管理通道；点通道在对话框里编辑。"
            >
                <FieldRow
                    label="启用 Webhook"
                    description={
                        draft.botOfflineWebHookNotice
                            ? enabledChannelCount > 0
                                ? `当前 ${enabledChannelCount} 个通道会在掉线时发送`
                                : '已开启，但还没有可用通道'
                            : '关闭时折叠配置，通道会保留'
                    }
                    isLast={!draft.botOfflineWebHookNotice}
                >
                    <Switch
                        checked={draft.botOfflineWebHookNotice}
                        onCheckedChange={(v) =>
                            patchDraft({ botOfflineWebHookNotice: v })
                        }
                    />
                </FieldRow>

                {draft.botOfflineWebHookNotice ? (
                    channels.length === 0 ? (
                        <FieldRow
                            label="推送通道"
                            description="还没有通道；点右侧添加，在对话框里填地址与模板"
                            isLast
                        >
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={openCreate}
                            >
                                <ActionMotionIcon icon={Plus} size={14} />
                                添加
                            </Button>
                        </FieldRow>
                    ) : (
                        <>
                            {channels.map((ch) => {
                                const ready = ch.enabled && !!ch.url.trim();
                                const status = !ch.enabled
                                    ? '已关闭'
                                    : !ch.url.trim()
                                      ? '缺地址'
                                      : ch.method || 'POST';
                                return (
                                    <FieldRow
                                        key={ch.id}
                                        label={channelDisplayName(ch)}
                                        description={`${channelSummary(ch)} · ${status}`}
                                        isLast={false}
                                    >
                                        <div className="flex items-center gap-1.5">
                                            <span
                                                className={cn(
                                                    'mr-1 h-1.5 w-1.5 shrink-0 rounded-full',
                                                    ready
                                                        ? 'bg-success'
                                                        : 'bg-text-tertiary/45',
                                                )}
                                                aria-hidden
                                            />
                                            {!ch.enabled ? (
                                                <Badge
                                                    tone="neutral"
                                                    appearance="soft"
                                                >
                                                    已关闭
                                                </Badge>
                                            ) : !ch.url.trim() ? (
                                                <Badge
                                                    tone="warning"
                                                    appearance="soft"
                                                >
                                                    缺地址
                                                </Badge>
                                            ) : null}
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                aria-label={`编辑 ${channelDisplayName(ch)}`}
                                                onClick={() => openEdit(ch)}
                                            >
                                                <ActionMotionIcon
                                                    icon={Pencil}
                                                    size={14}
                                                />
                                            </Button>
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                aria-label={`删除 ${channelDisplayName(ch)}`}
                                                onClick={() =>
                                                    setDeleteId(ch.id)
                                                }
                                            >
                                                <ActionMotionIcon
                                                    icon={Trash2}
                                                    size={14}
                                                />
                                            </Button>
                                        </div>
                                    </FieldRow>
                                );
                            })}
                            <FieldRow
                                label="添加通道"
                                description={`已有 ${channels.length} 个；继续添加可配置多路推送`}
                                isLast
                            >
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    onClick={openCreate}
                                >
                                    <ActionMotionIcon icon={Plus} size={14} />
                                    添加
                                </Button>
                            </FieldRow>
                        </>
                    )
                ) : null}
            </SettingsSection>

            <SettingsSection
                title={
                    <span className="flex items-center gap-2">
                        邮件通知
                        {draft.botOfflineEmailNotice && !emailIsReady(emailDraft) ? (
                            <Badge tone="warning" appearance="soft">待完善</Badge>
                        ) : null}
                    </span>
                }
                description="通过 SMTP 发 HTML 邮件。连接信息收敛到对话框，便于一次完成配置。"
            >
                <FieldRow
                    label="启用邮件"
                    description={
                        draft.botOfflineEmailNotice
                            ? emailIsReady(emailDraft)
                                ? `已就绪 · ${emailSummary(emailDraft)}`
                                : emailSummary(emailDraft)
                            : '关闭时折叠配置，已填内容会保留'
                    }
                    isLast
                >
                    <div className="flex items-center gap-2">
                        {draft.botOfflineEmailNotice ? (
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={openEmailEditor}
                            >
                                <ActionMotionIcon icon={Pencil} size={14} />
                                配置…
                            </Button>
                        ) : null}
                        <Switch
                            checked={draft.botOfflineEmailNotice}
                            onCheckedChange={(v) =>
                                patchDraft({ botOfflineEmailNotice: v })
                            }
                        />
                    </div>
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title={
                    <span className="flex items-center gap-2">
                        OneBot 通知
                        {draft.onebotNoticeEnabled && !oneBotIsReady(oneBotDraft) ? (
                            <Badge tone="warning" appearance="soft">待完善</Badge>
                        ) : null}
                    </span>
                }
                description="本机掉线用本机发送方；远端掉线由该机 ncd-watch 用同机发送方。不会跨服务器调用 OneBot。"
            >
                <FieldRow
                    label="启用 OneBot 通知"
                    description={
                        draft.onebotNoticeEnabled
                            ? oneBotIsReady(oneBotDraft)
                                ? `已就绪 · ${oneBotSummary(oneBotDraft)}`
                                : oneBotSummary(oneBotDraft)
                            : '关闭时折叠配置，已填内容会保留'
                    }
                    isLast
                >
                    <div className="flex items-center gap-2">
                        {draft.onebotNoticeEnabled ? (
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={openOneBotEditor}
                            >
                                <ActionMotionIcon icon={Pencil} size={14} />
                                配置…
                            </Button>
                        ) : null}
                        <Switch
                            checked={draft.onebotNoticeEnabled}
                            onCheckedChange={(v) =>
                                patchDraft({ onebotNoticeEnabled: v })
                            }
                        />
                    </div>
                </FieldRow>
            </SettingsSection>

            <SettingsSection
                title="应用内提示条"
                description="错误类始终需手动关闭；下面控制说明 / 成功 / 警告是否自动消失"
            >
                <FieldRow label="说明" description="蓝色提示">
                    <InfoBarDismissSliderPresence
                        visible={draft.infoBarDismissInfoEnabled}
                    >
                        <InfoBarDismissDurationSlider
                            value={draft.infoBarDismissInfoMs}
                            defaultMs={
                                DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissInfoMs
                            }
                            onChange={(v) =>
                                patchDraft({ infoBarDismissInfoMs: v })
                            }
                        />
                    </InfoBarDismissSliderPresence>
                    <Switch
                        checked={draft.infoBarDismissInfoEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ infoBarDismissInfoEnabled: v })
                        }
                    />
                </FieldRow>
                <FieldRow label="成功" description="绿色提示">
                    <InfoBarDismissSliderPresence
                        visible={draft.infoBarDismissSuccessEnabled}
                    >
                        <InfoBarDismissDurationSlider
                            value={draft.infoBarDismissSuccessMs}
                            defaultMs={
                                DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissSuccessMs
                            }
                            onChange={(v) =>
                                patchDraft({ infoBarDismissSuccessMs: v })
                            }
                        />
                    </InfoBarDismissSliderPresence>
                    <Switch
                        checked={draft.infoBarDismissSuccessEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ infoBarDismissSuccessEnabled: v })
                        }
                    />
                </FieldRow>
                <FieldRow label="警告" description="橙色提示" isLast>
                    <InfoBarDismissSliderPresence
                        visible={draft.infoBarDismissWarningEnabled}
                    >
                        <InfoBarDismissDurationSlider
                            value={draft.infoBarDismissWarningMs}
                            defaultMs={
                                DEFAULT_INFOBAR_DISMISS_WHEN_ENABLED.infoBarDismissWarningMs
                            }
                            onChange={(v) =>
                                patchDraft({ infoBarDismissWarningMs: v })
                            }
                        />
                    </InfoBarDismissSliderPresence>
                    <Switch
                        checked={draft.infoBarDismissWarningEnabled}
                        onCheckedChange={(v) =>
                            patchDraft({ infoBarDismissWarningEnabled: v })
                        }
                    />
                </FieldRow>
            </SettingsSection>

            {/* 投递历史 Dialog */}
            <Dialog
                open={historyDialogOpen}
                onOpenChange={setHistoryDialogOpen}
            >
                <DialogContent
                    size="lg"
                    dismissOnOutsideClick={false}
                    hideClose
                >
                    <DialogHeader>
                        <div className="flex min-w-0 items-center gap-2">
                            <DialogTitle>最近投递</DialogTitle>
                            <Badge tone="neutral" appearance="soft">
                                {history.length} 条
                            </Badge>
                        </div>
                        <DialogDescription>
                            本次运行的通知结果；重启应用后自动清空。
                        </DialogDescription>
                    </DialogHeader>

                    <div className="min-h-0 overflow-hidden rounded-sm border border-border-subtle bg-field">
                        {history.length === 0 ? (
                            <div className="flex min-h-48 flex-col justify-center px-5 py-8 text-center">
                                <p className="text-[13px] font-medium text-text-secondary">
                                    暂无投递记录
                                </p>
                                <p className="mx-auto mt-1.5 max-w-sm text-[12px] leading-relaxed text-text-tertiary">
                                    Bot 掉线、恢复或异常退出后，这里会记录桌面通知和各推送通道的结果。
                                </p>
                            </div>
                        ) : (
                            <ul className="max-h-[min(55vh,32rem)] divide-y divide-border-subtle/70 overflow-y-auto">
                                {history.map((h, i) => (
                                    <li
                                        key={`${h.at}-${h.bot_id}-${i}`}
                                        className="space-y-2 px-4 py-3"
                                    >
                                        <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                                            <div className="flex min-w-0 items-center gap-2">
                                                <span className="truncate text-[13px] font-medium text-text">
                                                    {h.botName || h.bot_id}
                                                </span>
                                                <Badge
                                                    tone={h.debounced ? 'warning' : 'neutral'}
                                                    appearance="soft"
                                                >
                                                    {h.debounced
                                                        ? '已合并重复提醒'
                                                        : historyKindLabel(h.kind)}
                                                </Badge>
                                            </div>
                                            <span className="shrink-0 text-[11px] tabular-nums text-text-tertiary">
                                                {h.at}
                                            </span>
                                        </div>
                                        {!h.debounced ? (
                                            <div className="flex flex-wrap gap-1.5">
                                                <DeliveryResultBadge label="桌面" value={h.toast} />
                                                <DeliveryResultBadge label="Webhook" value={h.webhook} />
                                                <DeliveryResultBadge label="邮件" value={h.email} />
                                                <DeliveryResultBadge label="OneBot" value={h.onebot} />
                                            </div>
                                        ) : null}
                                        {h.note ? (
                                            <p className="text-[12px] leading-relaxed text-text-tertiary">
                                                {h.note}
                                            </p>
                                        ) : null}
                                    </li>
                                ))}
                            </ul>
                        )}
                    </div>

                    <DialogFooter>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={historyLoading}
                            onClick={() => void refreshHistory()}
                        >
                            {historyLoading ? '刷新中…' : '刷新'}
                        </Button>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={history.length === 0}
                            onClick={async () => {
                                await settingsService.clearOfflineDeliveryHistory();
                                setHistory([]);
                            }}
                        >
                            清空记录
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            onClick={() => setHistoryDialogOpen(false)}
                        >
                            完成
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            <WebhookChannelEditorDialog
                open={editor !== null}
                working={editorWorking}
                testingKey={testing}
                onOpenChange={(open) => {
                    if (!open) closeEditor();
                }}
                onExited={() => setEditorMount(null)}
                onPatchDraft={patchEditorDraft}
                onSave={saveEditor}
                onTest={(channelId) => {
                    void runWebhookTest(channelId);
                }}
            />

            <EmailEditorDialog
                open={emailEditorOpen}
                draft={emailDraft}
                preset={emailPreset}
                onOpenChange={(open) => {
                    setEmailEditorOpen(open);
                    if (!open) setEmailEditorDraft(null);
                }}
                onPresetChange={setEmailPreset}
                onDraftChange={(patch) =>
                    setEmailEditorDraft((current) =>
                        current ? { ...current, ...patch } : current,
                    )
                }
                onSave={saveEmailEditor}
            />

            {/* OneBot Dialog：左发送方主列表，右栏目标 + 消息上下叠 */}
            <Dialog
                open={oneBotEditorOpen}
                onOpenChange={(open) => {
                    setOneBotEditorOpen(open);
                    if (!open) setOneBotEditorDraft(null);
                }}
            >
                <DialogContent size="sheetWide" dismissOnOutsideClick={false}>
                    <DialogHeader className="shrink-0">
                        <DialogTitle>配置 OneBot 通知</DialogTitle>
                        <DialogDescription>
                            按主机勾选发送方：本机供 Desktop，远端供同机 ncd-watch。目标与模板全局共用。
                        </DialogDescription>
                    </DialogHeader>

                    {/* min-h 抬高 sheetWide 内容高度，避免矮对话框里目标区被压扁 */}
                    <div className="grid min-h-[min(72dvh,40rem)] flex-1 grid-cols-1 gap-3 overflow-y-auto lg:grid-cols-[minmax(0,1.15fr)_minmax(18rem,0.85fr)] lg:overflow-hidden">
                        {/* 左：发送方列表（主焦点） */}
                        <div className="flex min-h-[22rem] min-w-0 flex-col lg:h-full lg:min-h-0">
                            <OneBotMessengerPicker
                                selected={oneBotDraft.onebotMessengerBotIds}
                                candidates={oneBotCandidates}
                                loading={oneBotCandidatesLoading}
                                enablingId={oneBotEnablingId}
                                onChange={(onebotMessengerBotIds) =>
                                    setOneBotEditorDraft((current) =>
                                        current
                                            ? {
                                                  ...current,
                                                  onebotMessengerBotIds,
                                              }
                                            : current,
                                    )
                                }
                                onEnsureHttp={(botId) => {
                                    void ensureOneBotHttp(botId);
                                }}
                            />
                        </div>

                        {/* 右：接收目标（上）+ 消息内容（下） */}
                        <div className="flex min-h-0 min-w-0 flex-col gap-3 lg:h-full lg:overflow-hidden">
                            <section className="shrink-0 space-y-2.5 rounded-sm border border-border-subtle bg-field p-3">
                                <div className="flex items-center justify-between gap-2">
                                    <p className="text-[13px] font-medium text-text">
                                        接收目标
                                    </p>
                                    {oneBotDraft.onebotTargetIds.length > 0 ? (
                                        <Badge tone="neutral" appearance="soft">
                                            {oneBotDraft.onebotTargetIds.length}{' '}
                                            个
                                        </Badge>
                                    ) : null}
                                </div>
                                <div className="space-y-2">
                                    <div className="w-full max-w-[10rem]">
                                        <Select
                                            value={oneBotDraft.onebotTargetType}
                                            onValueChange={(onebotTargetType) =>
                                                setOneBotEditorDraft((current) =>
                                                    current
                                                        ? {
                                                              ...current,
                                                              onebotTargetType,
                                                          }
                                                        : current,
                                                )
                                            }
                                            items={[...ONEBOT_TARGET_ITEMS]}
                                        />
                                    </div>
                                    <TargetIdChipsInput
                                        values={oneBotDraft.onebotTargetIds}
                                        className="min-h-[6.5rem] max-h-[9.5rem]"
                                        placeholder={
                                            oneBotDraft.onebotTargetType ===
                                            'group'
                                                ? '群号，逗号添加多个'
                                                : 'QQ 号，逗号添加多个'
                                        }
                                        onChange={(onebotTargetIds) =>
                                            setOneBotEditorDraft((current) =>
                                                current
                                                    ? {
                                                          ...current,
                                                          onebotTargetIds,
                                                      }
                                                    : current,
                                            )
                                        }
                                    />
                                </div>
                            </section>

                            <div className="flex min-h-[16rem] min-w-0 flex-1 flex-col lg:min-h-0">
                                <OneBotMessageEditor
                                    value={oneBotDraft.onebotMessageTemplate}
                                    onChange={(onebotMessageTemplate) =>
                                        setOneBotEditorDraft((current) =>
                                            current
                                                ? {
                                                      ...current,
                                                      onebotMessageTemplate,
                                                  }
                                                : current,
                                        )
                                    }
                                    onReset={() =>
                                        setOneBotEditorDraft((current) =>
                                            current
                                                ? {
                                                      ...current,
                                                      onebotMessageTemplate:
                                                          DEFAULT_ONEBOT_MESSAGE,
                                                  }
                                                : current,
                                        )
                                    }
                                />
                            </div>
                        </div>
                    </div>

                    <DialogFooter className="mt-3 shrink-0">
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => setOneBotEditorOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            onClick={saveOneBotEditor}
                        >
                            完成
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* 删除确认 */}
            <Dialog
                open={deleteId !== null}
                onOpenChange={(open) => {
                    if (!open) setDeleteId(null);
                }}
            >
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>删除此通道？</DialogTitle>
                        <DialogDescription>
                            {deleteTarget
                                ? `将删除「${channelDisplayName(deleteTarget)}」。写入草稿后需保存设置才会落盘。`
                                : '写入草稿后需保存设置才会落盘。'}
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => setDeleteId(null)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            variant="danger"
                            size="sm"
                            onClick={confirmDelete}
                        >
                            删除
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </SettingsTabSections>
    );
}
