import { chatService } from '../../core/services/chat.service';
import { qqFaceService } from '../../core/services/qq-face.service';
import {
    inlineImageService,
    isInlineImageReference,
} from '../../core/services/inline-image.service';
import { qqFaceLarge } from '../../core/domain/chat/qqFaces';
import { createChatMediaService } from '../../core/services/chat-media.service';
import { fileSegmentName, fileSegmentSource } from '../../core/domain/chat/groupFiles';
import { recoverDraft } from '../../core/domain/chat/recoverDraft';
import { markdownContent } from '../../core/domain/debug/markdown';
import {
    accountKey,
    ingestMessages,
    contactFromKey,
    setDraft,
    addPending,
    settleSend,
    record,
    id,
    text,
    EMPTY_DRAFT,
    type Account,
    type Contact,
    type Draft,
    type Message,
    type SessionKey,
} from '../../core/domain/chat/model';
import type { ReadingAnchor } from '../../core/domain/chat/messageWorkingSet';
import { buildMessageSegments } from '../../core/domain/debug/composerModel';
import { localFileTokenFor, LOCAL_FILE_PREFIX } from '../../core/domain/debug/streamActions';
import { debugErrorCopy } from '../../core/domain/debug/errorCopy';
import { errorText } from '../../core/domain/errors';
import type { Segment } from '../../core/domain/debug/segments';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { DebugCallResponse } from '../../core/ipc/generated/debug/DebugCallResponse';
import {
    canForwardMessage,
    canRecallMessage,
    canRepeatMessage,
    ChatForwardError,
    FORWARD_MESSAGE_LIMIT,
    type ForwardMode,
} from '../../core/domain/chat/messageTransfer';
import type { ChatSnapshot, ChatTransport } from './chatStore.types';
import type { ChatMediaCodec } from './chatMediaCodec';
import { dataOf } from './chatProtocol';

interface ChatActionHost {
    getSnapshot(): ChatSnapshot;
    getTarget(): DebugTarget;
    getEpoch(): number;
    transport: ChatTransport;
    media: ChatMediaCodec;
    update(patch: Partial<ChatSnapshot>): void;
    account(account: Account): void;
    peer(key: SessionKey): string | number;
    currentAnchor(key: SessionKey): ReadingAnchor | undefined;
    draft(key: SessionKey, draft: Draft): void;
    releaseIfIdle(): void;
}
type SendOperation = {
    action: string;
    params: unknown;
    segments: Segment[];
    resolveImages?: boolean;
    sourceMessageId?: string;
};

/** 协调单账号的发送、重试、撤回和转发，沿用所属 store 的连接代次。 */
export class ChatMessageActions {
    private sends = new Map<SessionKey, number>();
    private recalls = new Map<string, number>();
    constructor(private host: ChatActionHost) {}
    private get snapshot() {
        return this.host.getSnapshot();
    }
    private get target() {
        return this.host.getTarget();
    }
    private get epoch() {
        return this.host.getEpoch();
    }
    async send(key: SessionKey) {
        if (
            this.sends.get(key) === this.epoch ||
            !this.target.running ||
            this.target.online === false ||
            this.snapshot.connection.state !== 'connected'
        )
            return;
        const draft = this.snapshot.account.drafts[key] ?? EMPTY_DRAFT;
        if (!draft.text.trim() && !draft.attachments.length) return;
        const group = key.startsWith('group:');
        const peer = { [group ? 'group_id' : 'user_id']: this.host.peer(key) };
        const segments = buildMessageSegments(draft.text, draft.mentions ?? []);
        if (draft.reply) segments.unshift({ type: 'reply', data: { id: draft.reply.id } });
        for (const attachment of draft.attachments) {
            if (attachment.type === 'face')
                segments.push({ type: 'face', data: { id: attachment.id } });
            if (attachment.type === 'image')
                segments.push({
                    type: 'image',
                    data: {
                        file:
                            isInlineImageReference(attachment.path) ||
                            /^(base64:\/\/|https?:\/\/)/i.test(attachment.path)
                                ? attachment.path
                                : localFileTokenFor(attachment.path),
                        name: attachment.name,
                        sub_type: attachment.subType ?? 0,
                    },
                });
        }
        const operations: SendOperation[] = [];
        if (segments.some((s) => s.type !== 'reply'))
            operations.push({
                action: group ? 'send_group_msg' : 'send_private_msg',
                params: { ...peer, message: segments },
                segments,
            });
        for (const file of draft.attachments)
            if (file.type === 'file')
                operations.push({
                    action: group ? 'upload_group_file' : 'upload_private_file',
                    params: {
                        ...peer,
                        file: localFileTokenFor(file.path),
                        name: file.name,
                        upload_file: true,
                    },
                    segments: [
                        {
                            type: 'file',
                            data: { name: file.name, file: localFileTokenFor(file.path) },
                        },
                    ],
                });
        await this.runSends(key, operations);
    }

    async command(key: SessionKey, value: string, replyId?: string): Promise<void> {
        this.requireConnected();
        if (!value.trim()) throw new Error('按钮指令为空');
        if (this.sends.get(key) === this.epoch) throw new Error('此会话仍有消息正在发送');
        const epoch = this.epoch;
        const group = key.startsWith('group:');
        const segments = buildMessageSegments(value, []);
        if (replyId)
            segments.unshift({ type: 'reply', data: { id: this.protocolMessageId(replyId) } });
        this.sends.set(key, epoch);
        try {
            await this.runSends(
                key,
                [
                    {
                        action: group ? 'send_group_msg' : 'send_private_msg',
                        params: {
                            [group ? 'group_id' : 'user_id']: this.host.peer(key),
                            message: segments,
                        },
                        segments,
                    },
                ],
                undefined,
                true,
                true,
                '发送',
            );
        } finally {
            if (this.sends.get(key) === epoch) this.sends.delete(key);
            this.host.releaseIfIdle();
        }
    }

    async cancelSend(messageKey: string): Promise<void> {
        const message = this.findMessage(messageKey);
        if (!message?.requestId || message.status !== 'sending') return;
        const progress = this.snapshot.sendProgress[message.requestId];
        if (!progress || progress.stage === 'calling') return;
        await chatService.cancel(message.requestId);
    }

    async poke(key: SessionKey, userId: string) {
        if (
            !this.target.running ||
            this.target.online === false ||
            this.snapshot.connection.state !== 'connected'
        )
            return;
        const group = key.startsWith('group:');
        dataOf(
            await this.host.transport.call(
                this.target.bot_id,
                group ? 'group_poke' : 'friend_poke',
                group ? { group_id: this.host.peer(key), user_id: userId } : { user_id: userId },
            ),
        );
        // 成功后乐观上墙；后端回显的 poke 通知在 ingestMessage 里按时间窗去重
        const echo: Record<string, unknown> = {
            notice_type: 'notify',
            sub_type: 'poke',
            user_id: this.snapshot.account.selfId,
            target_id: userId,
            time: Math.floor(Date.now() / 1000),
        };
        if (group) echo.group_id = this.host.peer(key);
        this.host.account(
            ingestMessages(
                this.snapshot.account,
                [echo],
                false,
                null,
                this.host.currentAnchor(key),
            ),
        );
    }
    async retry(messageKey: string) {
        const message =
            this.snapshot.account.messages.find((m) => m.key === messageKey) ??
            this.snapshot.account.archiveMessages?.find((m) => m.key === messageKey);
        if (!message?.mine || message.status !== 'failed' || message.recalled) return;
        if (message.segments.some((segment) => segment.type === 'forward')) return;
        const key = message.session;
        if (
            !this.target.running ||
            this.target.online === false ||
            this.snapshot.connection.state !== 'connected'
        )
            throw new Error('聊天未连接，请连接后重发');
        if (this.sends.get(key) === this.epoch) return;
        const group = key.startsWith('group:');
        const peer = { [group ? 'group_id' : 'user_id']: this.host.peer(key) };
        const segments = message.segments.map((segment) => {
            if (segment.type === 'image' || segment.type === 'mface') {
                if (
                    !text(segment.data.local_file) &&
                    !text(segment.data.file) &&
                    !text(segment.data.url) &&
                    !text(segment.data.file_id) &&
                    !id(segment.data.emoji_id)
                )
                    throw new Error('原附件已不可用，请重新添加后发送');
                return { ...segment, data: { ...segment.data } };
            }
            if (segment.type !== 'file') return segment;
            const file =
                text(segment.data.local_file) || text(segment.data.file) || text(segment.data.url);
            if (!/^(ncd-local-file:\/\/|base64:\/\/|https?:\/\/).+/i.test(file))
                throw new Error('原附件已不可用，请重新添加后发送');
            // local_file 只服务本地预览与选源，不下发给协议。
            const data: Record<string, unknown> = { ...segment.data, file };
            delete data.local_file;
            return { ...segment, data };
        });
        if (!segments.length || segments.every((s) => s.type === 'reply'))
            throw new Error('原消息内容已不可用');
        const file =
            segments.length === 1 && segments[0].type === 'file' ? segments[0].data : undefined;
        const operation: SendOperation = file
            ? {
                  action: group ? 'upload_group_file' : 'upload_private_file',
                  params: { ...peer, file: file.file, name: file.name, upload_file: true },
                  segments,
              }
            : {
                  action: group ? 'send_group_msg' : 'send_private_msg',
                  params: { ...peer, message: segments },
                  segments,
                  resolveImages: segments.some(
                      (segment) => segment.type === 'image' || segment.type === 'mface',
                  ),
                  sourceMessageId: message.id,
              };
        if (!this.snapshot.account.messages.some((row) => row.key === message.key))
            this.host.account({
                ...this.snapshot.account,
                messages: [...this.snapshot.account.messages, { ...message, status: 'sending' }],
            });
        await this.runSends(key, [operation], message.key);
    }
    async recall(messageKey: string): Promise<void> {
        const message = this.findMessage(messageKey);
        if (
            !message ||
            !canRecallMessage(message) ||
            message.senderId !== this.snapshot.account.selfId
        )
            return;
        this.requireConnected();
        const epoch = this.epoch;
        if (this.recalls.get(messageKey) === epoch) return;
        const scope = accountKey(this.target.bot_id, String(this.target.qq_id));
        this.recalls.set(messageKey, epoch);
        try {
            const response = await this.host.transport.call(this.target.bot_id, 'delete_msg', {
                message_id: this.protocolMessageId(message.id!),
            });
            if (!this.actionIsCurrent(epoch, scope)) return;
            dataOf(response);
            const state = this.snapshot.account;
            const conversation = state.conversations[message.session];
            const latest = (state.archiveMessages ?? state.messages)
                .filter((row) => row.session === message.session)
                .at(-1);
            const mark = (row: Message) =>
                row.session === message.session && row.id === message.id
                    ? { ...row, recalled: true }
                    : row;
            this.host.account({
                ...state,
                messages: state.messages.map(mark),
                archiveMessages: state.archiveMessages?.map(mark),
                conversations:
                    conversation && latest?.id === message.id && message.at >= conversation.lastAt
                        ? {
                              ...state.conversations,
                              [message.session]: { ...conversation, preview: '消息已撤回' },
                          }
                        : state.conversations,
            });
        } catch (error) {
            if (this.actionIsCurrent(epoch, scope)) throw error;
        } finally {
            if (this.recalls.get(messageKey) === epoch) this.recalls.delete(messageKey);
            this.host.releaseIfIdle();
        }
    }
    async repeat(messageKey: string): Promise<void> {
        const message = this.findMessage(messageKey);
        if (!message || !canRepeatMessage(message)) return;
        this.requireConnected();
        if (this.sends.get(message.session) === this.epoch) return;
        const segments = message.segments.map((segment) => {
            if (segment.type === 'markdown' && segment.data.content === undefined) {
                const content = markdownContent(segment.data);
                if (!content) throw new Error('原 Markdown 内容已不可用');
                return { ...segment, data: { ...segment.data, content } };
            }
            return { ...segment, data: { ...segment.data } };
        });
        const group = message.session.startsWith('group:');
        await this.runSends(
            message.session,
            [
                {
                    action: group ? 'send_group_msg' : 'send_private_msg',
                    params: {
                        [group ? 'group_id' : 'user_id']: this.host.peer(message.session),
                        message: segments,
                    },
                    segments,
                    resolveImages: true,
                    sourceMessageId: message.id,
                },
            ],
            undefined,
            true,
        );
    }
    async forward(
        messageKeys: readonly string[],
        contact: Contact,
        mode: ForwardMode = 'merged',
        onProgress?: (sent: number, total: number) => void,
    ): Promise<void> {
        this.requireConnected();
        const keys = [...new Set(messageKeys)];
        if (!keys.length || keys.length > FORWARD_MESSAGE_LIMIT)
            throw new Error(`一次可转发 1–${FORWARD_MESSAGE_LIMIT} 条消息`);
        const messages = keys.map((key) => this.findMessage(key));
        if (messages.some((message) => !message || !canForwardMessage(message)))
            throw new Error('所选消息已不可转发，请重新选择');
        const source = (messages as Message[]).sort((a, b) => a.at - b.at);
        if (source.some((message) => message.session !== source[0].session))
            throw new Error('请选择同一会话中的消息');
        const known =
            this.snapshot.contacts.find((row) => row.key === contact.key) ??
            this.snapshot.account.conversations[contact.key];
        if (!known || known.id !== contact.id || known.type !== contact.type)
            throw new Error('转发目标不属于当前账号');
        const epoch = this.epoch;
        if (this.sends.get(contact.key) === epoch) throw new Error('该会话仍在发送，请稍后转发');
        if (mode === 'individual') {
            const state = this.snapshot.account;
            this.host.account({
                ...state,
                conversations: {
                    ...state.conversations,
                    [contact.key]: {
                        ...(state.conversations[contact.key] ?? {
                            unread: 0,
                            pinned: false,
                            lastAt: 0,
                            preview: '',
                        }),
                        ...known,
                    },
                },
            });
            this.sends.set(contact.key, epoch);
            let sent = 0;
            try {
                for (const message of source) {
                    this.requireConnected();
                    if (epoch !== this.epoch)
                        throw new ChatForwardError(
                            `连接已改变，已转发 ${sent}/${source.length} 条，请查看目标会话`,
                            true,
                        );
                    const segments = message.segments.filter(
                        (segment) => segment.type !== 'reply' && segment.type !== 'file',
                    );
                    const operations: SendOperation[] = [];
                    if (segments.length)
                        operations.push({
                            action:
                                contact.type === 'group' ? 'send_group_msg' : 'send_private_msg',
                            params: {
                                [contact.type === 'group' ? 'group_id' : 'user_id']: this.host.peer(
                                    contact.key,
                                ),
                                message: segments,
                            },
                            segments,
                            resolveImages: true,
                            sourceMessageId: message.id,
                        });
                    for (const segment of message.segments.filter(
                        (segment) => segment.type === 'file',
                    )) {
                        const sourceContact = contactFromKey(message.session);
                        const source = fileSegmentSource(
                            { ...segment.data, file_id: segment.data.file_id ?? message.fileId },
                            sourceContact,
                        );
                        const name = fileSegmentName(segment.data);
                        const local = text(segment.data.file);
                        const file = local.startsWith(LOCAL_FILE_PREFIX)
                            ? local
                            : source
                              ? localFileTokenFor(
                                    await chatService.cacheFile(this.target.bot_id, source, name),
                                )
                              : /^https?:\/\//i.test(local)
                                ? local
                                : '';
                        if (!file) throw new Error('文件消息缺少可转发的来源');
                        operations.push({
                            action:
                                contact.type === 'group'
                                    ? 'upload_group_file'
                                    : 'upload_private_file',
                            params: {
                                [contact.type === 'group' ? 'group_id' : 'user_id']: this.host.peer(
                                    contact.key,
                                ),
                                file,
                                name,
                                upload_file: true,
                            },
                            segments: [{ type: 'file', data: { name, file } }],
                        });
                    }
                    if (!operations.length) throw new Error('消息没有可转发的内容');
                    await this.runSends(contact.key, operations, undefined, true, true);
                    sent++;
                    onProgress?.(sent, source.length);
                }
            } catch (error) {
                const uncertain =
                    sent > 0 || (error instanceof ChatForwardError && error.uncertain);
                throw new ChatForwardError(
                    `已转发 ${sent}/${source.length} 条：${errorText(error)}${uncertain ? '。请先查看目标会话，避免重复转发' : ''}`,
                    uncertain,
                );
            } finally {
                if (this.sends.get(contact.key) === epoch) this.sends.delete(contact.key);
                this.host.releaseIfIdle();
            }
            return;
        }
        const scope = accountKey(this.target.bot_id, String(this.target.qq_id));
        const requestId = crypto.randomUUID();
        const peer = this.host.peer(contact.key);
        const content = source.map((message) => ({
            type: 'node',
            data: {
                user_id: message.senderId,
                nickname: message.senderName,
                message_id: message.id,
                time: Math.floor(message.at / 1000),
                content: message.segments,
            },
        }));
        const nodes = source.map((message) => ({
            type: 'node',
            data: { id: this.protocolMessageId(message.id!) },
        }));
        this.sends.set(contact.key, epoch);
        const state = this.snapshot.account;
        this.host.account(
            addPending(
                {
                    ...state,
                    conversations: {
                        ...state.conversations,
                        [contact.key]: {
                            ...(state.conversations[contact.key] ?? {
                                unread: 0,
                                pinned: false,
                                lastAt: 0,
                                preview: '',
                            }),
                            ...known,
                        },
                    },
                },
                contact.key,
                requestId,
                [{ type: 'forward', data: { content } }],
                Date.now(),
            ),
        );
        try {
            const group = contact.type === 'group';
            let response: DebugCallResponse;
            try {
                response = await this.host.transport.call(
                    this.target.bot_id,
                    group ? 'send_group_forward_msg' : 'send_private_forward_msg',
                    { [group ? 'group_id' : 'user_id']: peer, messages: nodes },
                    requestId,
                );
            } catch (error) {
                throw new ChatForwardError(
                    `转发结果待确认：${errorText(error)}。请先查看目标会话`,
                    true,
                );
            }
            if (!this.actionIsCurrent(epoch, scope))
                throw new ChatForwardError('连接已改变，转发结果待确认，请先查看目标会话', true);
            if (response.result.kind === 'err') {
                const error = response.result.error;
                const uncertain = ['timeout', 'cancelled', 'transport', 'internal'].includes(
                    error.kind,
                );
                const copy = debugErrorCopy(error);
                throw new ChatForwardError(
                    uncertain
                        ? `转发结果待确认，请先查看目标会话：${copy.title}`
                        : [copy.title, copy.detail].filter(Boolean).join('：'),
                    uncertain,
                );
            }
            const result = response.result.outcome;
            if (!result.ok)
                throw new ChatForwardError(result.wording || result.message || '转发失败');
            this.host.account(
                settleSend(this.snapshot.account, requestId, {
                    state: 'sent',
                    id: id(record(result.data).message_id) || undefined,
                }),
            );
        } catch (error) {
            if (this.actionIsCurrent(epoch, scope))
                this.host.account(
                    settleSend(this.snapshot.account, requestId, {
                        state:
                            error instanceof ChatForwardError && error.uncertain
                                ? 'unknown'
                                : 'failed',
                        error: errorText(error),
                    }),
                );
            throw error;
        } finally {
            if (this.sends.get(contact.key) === epoch) this.sends.delete(contact.key);
            this.host.releaseIfIdle();
        }
    }
    private findMessage(key: string): Message | undefined {
        return (
            this.snapshot.account.messages.find((message) => message.key === key) ??
            this.snapshot.account.archiveMessages?.find((message) => message.key === key)
        );
    }
    private requireConnected() {
        if (
            !this.target.running ||
            this.target.online === false ||
            this.snapshot.connection.state !== 'connected'
        )
            throw new Error('聊天未连接，请连接后操作');
    }
    private protocolMessageId(value: string): string | number {
        if (!/^-?\d+$/.test(value) || /^-?0+$/.test(value))
            throw new Error('此消息缺少有效协议标识');
        if (this.target.backend !== 'snowluma') return value;
        const numeric = Number(value);
        if (!Number.isSafeInteger(numeric)) throw new Error('此协议的消息标识超出可支持范围');
        return numeric;
    }
    private actionIsCurrent(epoch: number, scope: string): boolean {
        return (
            epoch === this.epoch &&
            scope === accountKey(this.target.bot_id, String(this.target.qq_id))
        );
    }
    private async runSends(
        key: SessionKey,
        operations: SendOperation[],
        retryKey?: string,
        preserveDraft = false,
        forwarding = false,
        operationLabel = '转发',
    ) {
        const epoch = this.epoch;
        const scope = accountKey(this.target.bot_id, String(this.target.qq_id));
        const created: string[] = [];
        let completedOperations = 0;
        let prepared: SendOperation[];
        try {
            prepared = operations.map((operation) => ({
                ...operation,
                segments: this.host.media.externalizeSegments(operation.segments, created),
            }));
        } catch (error) {
            for (const reference of created)
                inlineImageService.discard(
                    this.target.bot_id,
                    String(this.target.qq_id),
                    reference,
                );
            throw error;
        }
        this.sends.set(key, epoch);
        if (!retryKey && !preserveDraft) this.host.draft(key, EMPTY_DRAFT);
        try {
            for (const operation of prepared) {
                const requestId = crypto.randomUUID();
                this.host.account(
                    retryKey
                        ? {
                              ...this.snapshot.account,
                              messages: this.snapshot.account.messages.map((message) =>
                                  message.key === retryKey
                                      ? {
                                            ...message,
                                            requestId,
                                            id: undefined,
                                            fileId: undefined,
                                            segments: operation.segments,
                                            status: 'sending' as const,
                                            error: undefined,
                                        }
                                      : message,
                              ),
                          }
                        : addPending(
                              this.snapshot.account,
                              key,
                              requestId,
                              operation.segments,
                              Date.now(),
                          ),
                );
                if (!this.actionIsCurrent(epoch, scope)) {
                    this.host.account(
                        settleSend(this.snapshot.account, requestId, {
                            state: 'failed',
                            error: '发送已中止，此条尚未提交',
                        }),
                    );
                    if (forwarding)
                        throw new ChatForwardError(`连接已改变，${operationLabel}已停止`, true);
                    continue;
                }
                if (
                    this.target.backend === 'snowluma' &&
                    operation.segments.some((segment) => segment.type === 'face')
                ) {
                    try {
                        await qqFaceService.validate(
                            this.target,
                            operation.segments
                                .filter((segment) => segment.type === 'face')
                                .map((segment) => text(segment.data.id)),
                            (...args) => this.host.transport.call(...args),
                        );
                    } catch (error) {
                        if (this.actionIsCurrent(epoch, scope)) {
                            const failed = settleSend(this.snapshot.account, requestId, {
                                state: 'failed',
                                error: errorText(error),
                            });
                            const message = failed.messages.find(
                                (row) => row.requestId === requestId,
                            );
                            this.host.account(
                                !retryKey && !preserveDraft && message
                                    ? setDraft(
                                          failed,
                                          key,
                                          recoverDraft(message, failed.drafts[key] ?? EMPTY_DRAFT),
                                      )
                                    : failed,
                            );
                        }
                        if (forwarding)
                            throw new ChatForwardError(errorText(error), completedOperations > 0);
                        continue;
                    }
                    if (!this.actionIsCurrent(epoch, scope)) {
                        if (forwarding)
                            throw new ChatForwardError(`连接已改变，${operationLabel}已停止`, true);
                        continue;
                    }
                }
                let submitted = false;
                try {
                    let params = Array.isArray(record(operation.params).message)
                        ? { ...record(operation.params), message: operation.segments }
                        : operation.params;
                    if (operation.resolveImages) {
                        const media = createChatMediaService((...args) =>
                            this.host.transport.call(...args),
                        );
                        const target = { ...this.target };
                        const segments: Segment[] = [];
                        let imageIndex = 0;
                        for (const segment of operation.segments) {
                            if (!this.actionIsCurrent(epoch, scope)) break;
                            if (segment.type === 'record' || segment.type === 'video') {
                                const reference =
                                    segment.type === 'record'
                                        ? await media.record(target, segment.data)
                                        : await media.video(target, segment.data);
                                segments.push({
                                    type: segment.type,
                                    data: {
                                        file: reference.startsWith('data:')
                                            ? `base64://${reference.slice(reference.indexOf(',') + 1)}`
                                            : reference,
                                    },
                                });
                            } else
                                segments.push(
                                    segment.type === 'image' || segment.type === 'mface'
                                        ? await media.imageForSend(target, segment.data, {
                                              messageId: operation.sourceMessageId,
                                              imageIndex: imageIndex++,
                                          })
                                        : segment,
                                );
                        }
                        if (!this.actionIsCurrent(epoch, scope)) {
                            if (forwarding)
                                throw new ChatForwardError(
                                    `连接已改变，${operationLabel}已停止`,
                                    true,
                                );
                            continue;
                        }
                        params = { ...record(params), message: segments };
                    }
                    if (
                        (operation.resolveImages || retryKey) &&
                        this.target.backend === 'snowluma'
                    ) {
                        const segments = record(params).message;
                        if (Array.isArray(segments))
                            params = {
                                ...record(params),
                                message: segments.map((segment: Segment) => {
                                    const large =
                                        segment.type === 'face'
                                            ? qqFaceLarge(segment.data)
                                            : undefined;
                                    return large === undefined
                                        ? segment
                                        : { ...segment, data: { ...segment.data, large } };
                                }),
                            };
                    }
                    if (Array.isArray(record(params).message))
                        params = {
                            ...record(params),
                            message: this.host.media.materializeSegments(
                                record(params).message as Segment[],
                            ),
                        };
                    submitted = true;
                    const response = await this.host.transport.call(
                        this.target.bot_id,
                        operation.action,
                        params,
                        requestId,
                        (progress) => {
                            if (
                                !this.actionIsCurrent(epoch, scope) ||
                                progress.v !== 1 ||
                                progress.request_id !== requestId ||
                                !this.snapshot.account.messages.some(
                                    (message) =>
                                        message.requestId === requestId &&
                                        message.status === 'sending',
                                )
                            )
                                return;
                            this.host.update({
                                sendProgress: {
                                    ...this.snapshot.sendProgress,
                                    [requestId]: progress,
                                },
                            });
                        },
                    );
                    if (!this.actionIsCurrent(epoch, scope)) {
                        if (forwarding)
                            throw new ChatForwardError(`连接已改变，${operationLabel}已停止`, true);
                        continue;
                    }
                    if (response.result.kind === 'err') {
                        const error = response.result.error;
                        const unknown = ['timeout', 'cancelled', 'transport', 'internal'].includes(
                            error.kind,
                        );
                        const copy = debugErrorCopy(error);
                        this.host.account(
                            settleSend(this.snapshot.account, requestId, {
                                state: unknown ? 'unknown' : 'failed',
                                error: [copy.title, copy.detail].filter(Boolean).join('：'),
                            }),
                        );
                    } else {
                        const result = response.result.outcome;
                        if (
                            !result.ok &&
                            /system face.*(?:absent|incomplete)/i.test(
                                result.wording || result.message || '',
                            )
                        )
                            qqFaceService.invalidate(this.target);
                        this.host.account(
                            settleSend(this.snapshot.account, requestId, {
                                state: result.ok ? 'sent' : 'failed',
                                id: id(record(result.data).message_id) || undefined,
                                fileId: operation.action.startsWith('upload_')
                                    ? id(record(result.data).file_id) || undefined
                                    : undefined,
                                error: result.ok
                                    ? undefined
                                    : `${result.wording || result.message || '发送失败'}（错误码 ${result.retcode}）`,
                            }),
                        );
                    }
                } catch (error) {
                    if (this.actionIsCurrent(epoch, scope))
                        this.host.account(
                            settleSend(this.snapshot.account, requestId, {
                                state: submitted ? 'unknown' : 'failed',
                                error: errorText(error),
                            }),
                        );
                } finally {
                    if (
                        this.actionIsCurrent(epoch, scope) &&
                        this.snapshot.sendProgress[requestId]
                    ) {
                        const next = { ...this.snapshot.sendProgress };
                        delete next[requestId];
                        this.host.update({ sendProgress: next });
                    }
                }
                if (forwarding) {
                    const message = this.snapshot.account.messages.find(
                        (row) => row.requestId === requestId,
                    );
                    if (!this.actionIsCurrent(epoch, scope) || message?.status !== 'sent')
                        throw new ChatForwardError(
                            message?.error || `${operationLabel}结果待确认`,
                            completedOperations > 0 || message?.status !== 'failed',
                        );
                    completedOperations++;
                }
            }
        } finally {
            if (!forwarding && this.sends.get(key) === epoch) this.sends.delete(key);
            if (!forwarding) this.host.releaseIfIdle();
        }
    }
    isSending() {
        return this.sends.size > 0 || this.recalls.size > 0;
    }
}
