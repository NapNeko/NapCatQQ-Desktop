// 通知设置：掉线时如何通知你。
// 主列表只留开关 + 通道摘要；通道详情 / 消息模板收敛进 Dialog（对齐连接配置交互）。
// 区块 UI 拆在 ./notifications/；services 直连是 eslint 白名单，只留在本文件。
// 颜色只走语义 token，随主题切换。

import { useEffect, useState } from 'react';
import {
    createBlankWebhookChannel,
    DEFAULT_ONEBOT_MESSAGE,
    type WebhookChannelDraft,
} from '../../../core/domain/settings/offline-notify-defaults';
import { settingsService } from '../../../core/services/settings.service';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../../hooks/ui/pushErrorBar';
import { errorText } from '../../../core/domain/errors';
import type { OfflineDeliveryRecord } from '../../../core/ipc/generated/domain/OfflineDeliveryRecord';
import type { SettingsDraft } from '../settings-draft';
import { SettingsTabSections } from '../_shared';
import { NcdWatchRemoteSection } from './NcdWatchRemoteSection';
import { useFeatureEnabled } from '../../../hooks/preferences/featureTogglesStore';
import { DeliveryHistoryDialog } from './notifications/DeliveryHistoryDialog';
import { EmailEditorDialog, type EmailEditorDraft } from './notifications/EmailEditorDialog';
import { OneBotEditorDialog, type OneBotEditorDraft } from './notifications/OneBotEditorDialog';
import type { OneBotCandidate } from './notifications/OneBotMessengerPicker';
import {
    WebhookChannelEditorDialog,
    type ChannelEditorState,
} from './notifications/WebhookChannelEditorDialog';
import { DesktopNotifySection } from './notifications/DesktopNotifySection';
import { DeliveryBehaviorSection } from './notifications/DeliveryBehaviorSection';
import { WebhookChannelsSection } from './notifications/WebhookChannelsSection';
import { EmailChannelSection } from './notifications/EmailChannelSection';
import { OneBotChannelSection } from './notifications/OneBotChannelSection';
import { InfoBarDismissSection } from './notifications/InfoBarDismissSection';
import { DeleteChannelDialog } from './notifications/DeleteChannelDialog';
import { newChannelId } from './notifications/channel-utils';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
    /** 草稿未保存时禁用 ncd-watch 同步（避免写旧 Webhook） */
    settingsDirty?: boolean;
}

export function NotificationsTab({ draft, patchDraft, settingsDirty = false }: Props) {
    const [testing, setTesting] = useState<string | null>(null);
    const ncdWatchEnabled = useFeatureEnabled('ncdWatch');
    const [editor, setEditor] = useState<ChannelEditorState | null>(null);
    const [editorMount, setEditorMount] = useState<ChannelEditorState | null>(null);
    const [deleteId, setDeleteId] = useState<string | null>(null);
    const [emailEditorOpen, setEmailEditorOpen] = useState(false);
    const [emailEditorDraft, setEmailEditorDraft] = useState<EmailEditorDraft | null>(null);
    const [emailPreset, setEmailPreset] = useState('');
    const [oneBotEditorOpen, setOneBotEditorOpen] = useState(false);
    const [oneBotEditorDraft, setOneBotEditorDraft] = useState<OneBotEditorDraft | null>(null);
    const [oneBotCandidates, setOneBotCandidates] = useState<OneBotCandidate[]>([]);
    const [oneBotCandidatesLoading, setOneBotCandidatesLoading] = useState(false);
    const [oneBotEnablingId, setOneBotEnablingId] = useState<string | null>(null);
    const [history, setHistory] = useState<OfflineDeliveryRecord[]>([]);
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
        return <p className="text-[13px] text-text-tertiary">正在加载设置…</p>;
    }

    const channels = draft.webHookChannels;
    const deleteTarget = channels.find((c) => c.id === deleteId) ?? null;

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
        setEditor((cur) => (cur ? { ...cur, draft: { ...cur.draft, ...patch } } : cur));
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
            updateChannels(channels.map((c) => (c.id === editor.id ? nextDraft : c)));
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
            onebotTargetType: draft.onebotTargetType === 'group' ? 'group' : 'private',
            onebotTargetIds: targetIds,
            onebotMessageTemplate: draft.onebotMessageTemplate || DEFAULT_ONEBOT_MESSAGE,
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
                oneBotEditorDraft.onebotMessageTemplate.trim() || DEFAULT_ONEBOT_MESSAGE,
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
                    ? '远端配置已写入；保存后同步 ncd-watch。运行中会尽量热更新。'
                    : '若 Bot 正在运行，会热更新连接配置。';
            pushInfoBar({
                key: 'onebot-enable-http',
                tone: 'success',
                title: actionText,
                content: `${result.candidate.name || botId} · 端口 ${result.port}。${scopeHint}`,
            });
        } catch (err) {
            pushErrorBar({
                key: 'onebot-enable-http',
                title: '自动配置 HTTP 失败',
                raw: errorText(err),
            });
        } finally {
            setOneBotEnablingId(null);
        }
    };

    const runWebhookTest = async (channel: WebhookChannelDraft) => {
        setTesting(`webhook:${channel.id}`);
        try {
            await settingsService.testWebhook(channel.id, channel);
            pushInfoBar({
                key: 'offline-webhook-test',
                tone: 'success',
                title: '测试已发送',
                content: '到目标服务确认是否收到。',
            });
        } catch (err) {
            pushErrorBar({
                key: 'offline-webhook-test',
                title: '测试失败',
                raw: errorText(err),
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
        onebotTargetType: draft.onebotTargetType === 'group' ? 'group' : 'private',
        onebotTargetIds:
            draft.onebotTargetIds.length > 0
                ? draft.onebotTargetIds
                : draft.onebotTargetId > 0
                  ? [draft.onebotTargetId]
                  : [],
        onebotMessageTemplate: draft.onebotMessageTemplate || DEFAULT_ONEBOT_MESSAGE,
    };

    return (
        <SettingsTabSections>
            <DesktopNotifySection draft={draft} patchDraft={patchDraft} />

            {ncdWatchEnabled && <NcdWatchRemoteSection settingsDirty={settingsDirty} />}

            <DeliveryBehaviorSection
                draft={draft}
                patchDraft={patchDraft}
                historyCount={history.length}
                onOpenHistory={openHistoryDialog}
            />

            <WebhookChannelsSection
                draft={draft}
                patchDraft={patchDraft}
                channels={channels}
                onOpenCreate={openCreate}
                onOpenEdit={openEdit}
                onRequestDelete={setDeleteId}
            />

            <EmailChannelSection
                draft={draft}
                patchDraft={patchDraft}
                emailDraft={emailDraft}
                onOpenEditor={openEmailEditor}
            />

            <OneBotChannelSection
                draft={draft}
                patchDraft={patchDraft}
                oneBotDraft={oneBotDraft}
                onOpenEditor={openOneBotEditor}
            />

            <InfoBarDismissSection draft={draft} patchDraft={patchDraft} />

            <DeliveryHistoryDialog
                open={historyDialogOpen}
                history={history}
                loading={historyLoading}
                onOpenChange={setHistoryDialogOpen}
                onRefresh={() => {
                    void refreshHistory();
                }}
                onClear={() => {
                    void (async () => {
                        await settingsService.clearOfflineDeliveryHistory();
                        setHistory([]);
                    })();
                }}
            />

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
                onTest={(channel) => {
                    void runWebhookTest(channel);
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
                    setEmailEditorDraft((current) => (current ? { ...current, ...patch } : current))
                }
                onSave={saveEmailEditor}
            />

            <OneBotEditorDialog
                open={oneBotEditorOpen}
                draft={oneBotDraft}
                candidates={oneBotCandidates}
                candidatesLoading={oneBotCandidatesLoading}
                enablingId={oneBotEnablingId}
                onOpenChange={(open) => {
                    setOneBotEditorOpen(open);
                    if (!open) setOneBotEditorDraft(null);
                }}
                onDraftChange={(patch) =>
                    setOneBotEditorDraft((current) =>
                        current ? { ...current, ...patch } : current,
                    )
                }
                onEnsureHttp={(botId) => {
                    void ensureOneBotHttp(botId);
                }}
                onSave={saveOneBotEditor}
            />

            {/* 删除确认 */}
            <DeleteChannelDialog
                target={deleteTarget}
                onClose={() => setDeleteId(null)}
                onConfirm={confirmDelete}
            />
        </SettingsTabSections>
    );
}
