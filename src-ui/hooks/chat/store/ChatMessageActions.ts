import { chatService } from '../../../core/services/chat.service';
import { isInlineImageReference } from '../../../core/services/inline-image.service';
import { markdownContent } from '../../../core/domain/debug/markdown';
import {
    accountKey,
    ingestMessages,
    text,
    id,
    EMPTY_DRAFT,
    type SessionKey,
    type Message,
} from '../../../core/domain/chat/model';
import { buildMessageSegments } from '../../../core/domain/debug/composerModel';
import { localFileTokenFor } from '../../../core/domain/debug/streamActions';
import { canRecallMessage, canRepeatMessage } from '../../../core/domain/chat/messageTransfer';
import { dataOf } from './chatProtocol';
import type { ChatAccountState } from './ChatAccountState';
import type { ChatOperations } from './ChatOperations';
import type { ChatSendQueue, SendOperation } from './ChatSendQueue';

/** 将用户操作转换为发送任务；发送管线和转发各自独立。 */
export class ChatMessageActions {
    constructor(
        private state: ChatAccountState,
        private operations: ChatOperations,
        private queue: ChatSendQueue,
    ) {}
    async send(key: SessionKey) {
        if (
            this.operations.isSending(key, this.state.session.epoch) ||
            !this.state.session.target.running ||
            this.state.session.target.online === false ||
            this.state.snapshot.connection.state !== 'connected'
        )
            return;
        const draft = this.state.snapshot.account.drafts[key] ?? EMPTY_DRAFT;
        if (!draft.text.trim() && !draft.attachments.length) return;
        const group = key.startsWith('group:');
        const peer = { [group ? 'group_id' : 'user_id']: this.state.session.peer(key) };
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
        await this.queue.run(key, operations);
    }

    async command(key: SessionKey, value: string, replyId?: string): Promise<void> {
        this.state.requireConnected();
        if (!value.trim()) throw new Error('按钮指令为空');
        if (this.operations.isSending(key, this.state.session.epoch))
            throw new Error('此会话仍有消息正在发送');
        const epoch = this.state.session.epoch;
        const group = key.startsWith('group:');
        const segments = buildMessageSegments(value, []);
        if (replyId)
            segments.unshift({
                type: 'reply',
                data: { id: this.state.session.protocolMessageId(replyId) },
            });
        this.operations.startSend(key, epoch);
        try {
            await this.queue.run(
                key,
                [
                    {
                        action: group ? 'send_group_msg' : 'send_private_msg',
                        params: {
                            [group ? 'group_id' : 'user_id']: this.state.session.peer(key),
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
            this.operations.finishSend(key, epoch);
        }
    }

    async cancelSend(messageKey: string): Promise<void> {
        const message = this.state.findMessage(messageKey);
        if (!message?.requestId || message.status !== 'sending') return;
        const progress = this.state.snapshot.sendProgress[message.requestId];
        if (!progress || progress.stage === 'calling') return;
        await chatService.cancel(message.requestId);
    }

    async poke(key: SessionKey, userId: string) {
        if (
            !this.state.session.target.running ||
            this.state.session.target.online === false ||
            this.state.snapshot.connection.state !== 'connected'
        )
            return;
        const epoch = this.state.session.epoch;
        const scope = accountKey(
            this.state.session.target.bot_id,
            String(this.state.session.target.qq_id),
        );
        const group = key.startsWith('group:');
        try {
            const response = await this.state.session.transport.call(
                this.state.session.target.bot_id,
                group ? 'group_poke' : 'friend_poke',
                group
                    ? { group_id: this.state.session.peer(key), user_id: userId }
                    : { user_id: userId },
            );
            if (!this.state.session.isCurrent(epoch, scope)) return;
            dataOf(response);
        } catch (error) {
            if (!this.state.session.isCurrent(epoch, scope)) return;
            throw error;
        }
        // 成功后乐观上墙；后端回显的 poke 通知在 ingestMessage 里按时间窗去重
        const echo: Record<string, unknown> = {
            notice_type: 'notify',
            sub_type: 'poke',
            user_id: this.state.snapshot.account.selfId,
            target_id: userId,
            time: Math.floor(Date.now() / 1000),
        };
        if (group) echo.group_id = this.state.session.peer(key);
        this.state.account(
            ingestMessages(
                this.state.snapshot.account,
                [echo],
                false,
                null,
                this.state.timeline.currentAnchor(key),
            ),
        );
    }
    async retry(messageKey: string) {
        const message =
            this.state.snapshot.account.messages.find((m) => m.key === messageKey) ??
            this.state.snapshot.account.archiveMessages?.find((m) => m.key === messageKey);
        if (!message?.mine || message.status !== 'failed' || message.recalled) return;
        if (message.segments.some((segment) => segment.type === 'forward')) return;
        const key = message.session;
        if (
            !this.state.session.target.running ||
            this.state.session.target.online === false ||
            this.state.snapshot.connection.state !== 'connected'
        )
            throw new Error('聊天未连接，请连接后重发');
        if (this.operations.isSending(key, this.state.session.epoch)) return;
        const group = key.startsWith('group:');
        const peer = { [group ? 'group_id' : 'user_id']: this.state.session.peer(key) };
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
        if (!this.state.snapshot.account.messages.some((row) => row.key === message.key))
            this.state.account({
                ...this.state.snapshot.account,
                messages: [
                    ...this.state.snapshot.account.messages,
                    { ...message, status: 'sending' },
                ],
            });
        await this.queue.run(key, [operation], message.key);
    }
    async recall(messageKey: string): Promise<void> {
        const message = this.state.findMessage(messageKey);
        if (
            !message ||
            !canRecallMessage(message) ||
            message.senderId !== this.state.snapshot.account.selfId
        )
            return;
        this.state.requireConnected();
        const epoch = this.state.session.epoch;
        if (this.operations.isRecalling(messageKey, epoch)) return;
        const scope = accountKey(
            this.state.session.target.bot_id,
            String(this.state.session.target.qq_id),
        );
        this.operations.startRecall(messageKey, epoch);
        try {
            const response = await this.state.session.transport.call(
                this.state.session.target.bot_id,
                'delete_msg',
                {
                    message_id: this.state.session.protocolMessageId(message.id!),
                },
            );
            if (!this.state.session.isCurrent(epoch, scope)) return;
            dataOf(response);
            const state = this.state.snapshot.account;
            const conversation = state.conversations[message.session];
            const latest = (state.archiveMessages ?? state.messages)
                .filter((row) => row.session === message.session)
                .at(-1);
            const mark = (row: Message) =>
                row.session === message.session && row.id === message.id
                    ? { ...row, recalled: true }
                    : row;
            this.state.account({
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
            if (this.state.session.isCurrent(epoch, scope)) throw error;
        } finally {
            this.operations.finishRecall(messageKey, epoch);
        }
    }
    async repeat(messageKey: string): Promise<void> {
        const message = this.state.findMessage(messageKey);
        if (!message || !canRepeatMessage(message)) return;
        this.state.requireConnected();
        if (this.operations.isSending(message.session, this.state.session.epoch)) return;
        const segments = message.segments.map((segment) => {
            if (segment.type === 'markdown' && segment.data.content === undefined) {
                const content = markdownContent(segment.data);
                if (!content) throw new Error('原 Markdown 内容已不可用');
                return { ...segment, data: { ...segment.data, content } };
            }
            return { ...segment, data: { ...segment.data } };
        });
        const group = message.session.startsWith('group:');
        await this.queue.run(
            message.session,
            [
                {
                    action: group ? 'send_group_msg' : 'send_private_msg',
                    params: {
                        [group ? 'group_id' : 'user_id']: this.state.session.peer(message.session),
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
}
