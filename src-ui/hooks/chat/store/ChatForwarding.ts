import { chatService } from '../../../core/services/chat.service';
import { fileSegmentName, fileSegmentSource } from '../../../core/domain/chat/groupFiles';
import {
    accountKey,
    contactFromKey,
    addPending,
    settleSend,
    record,
    id,
    text,
    type Contact,
    type Message,
} from '../../../core/domain/chat/model';
import { localFileTokenFor, LOCAL_FILE_PREFIX } from '../../../core/domain/debug/streamActions';
import { debugErrorCopy } from '../../../core/domain/debug/errorCopy';
import { errorText } from '../../../core/domain/errors';
import type { DebugCallResponse } from '../../../core/ipc/generated/debug/DebugCallResponse';
import {
    canForwardMessage,
    ChatForwardError,
    FORWARD_MESSAGE_LIMIT,
    type ForwardMode,
} from '../../../core/domain/chat/messageTransfer';
import type { ChatAccountState } from './ChatAccountState';
import type { ChatOperations } from './ChatOperations';
import type { ChatSendQueue, SendOperation } from './ChatSendQueue';

/** 转发负责选择与目标校验、逐条/合并模式及部分成功语义，复用同一发送队列。 */
export class ChatForwarding {
    constructor(
        private state: ChatAccountState,
        private operations: ChatOperations,
        private queue: ChatSendQueue,
    ) {}
    async forward(
        messageKeys: readonly string[],
        contact: Contact,
        mode: ForwardMode = 'merged',
        onProgress?: (sent: number, total: number) => void,
    ): Promise<void> {
        this.state.requireConnected();
        const keys = [...new Set(messageKeys)];
        if (!keys.length || keys.length > FORWARD_MESSAGE_LIMIT)
            throw new Error(`一次可转发 1–${FORWARD_MESSAGE_LIMIT} 条消息`);
        const messages = keys.map((key) => this.state.findMessage(key));
        if (messages.some((message) => !message || !canForwardMessage(message)))
            throw new Error('所选消息已不可转发，请重新选择');
        const source = (messages as Message[]).sort((a, b) => a.at - b.at);
        if (source.some((message) => message.session !== source[0].session))
            throw new Error('请选择同一会话中的消息');
        const known =
            this.state.snapshot.contacts.find((row) => row.key === contact.key) ??
            this.state.snapshot.account.conversations[contact.key];
        if (!known || known.id !== contact.id || known.type !== contact.type)
            throw new Error('转发目标不属于当前账号');
        const epoch = this.state.session.epoch;
        const scope = accountKey(
            this.state.session.target.bot_id,
            String(this.state.session.target.qq_id),
        );
        if (this.operations.isSending(contact.key, epoch))
            throw new Error('该会话仍在发送，请稍后转发');
        if (mode === 'individual') {
            const state = this.state.snapshot.account;
            this.state.account({
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
            this.operations.startSend(contact.key, epoch);
            let sent = 0;
            const requireCurrent = () => {
                if (!this.state.session.isCurrent(epoch, scope))
                    throw new ChatForwardError('连接已改变，转发已停止', sent > 0);
                this.state.requireConnected();
            };
            try {
                for (const message of source) {
                    requireCurrent();
                    const segments = message.segments.filter(
                        (segment) => segment.type !== 'reply' && segment.type !== 'file',
                    );
                    const operations: SendOperation[] = [];
                    if (segments.length)
                        operations.push({
                            action:
                                contact.type === 'group' ? 'send_group_msg' : 'send_private_msg',
                            params: {
                                [contact.type === 'group' ? 'group_id' : 'user_id']:
                                    this.state.session.peer(contact.key),
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
                                    await chatService.cacheFile(
                                        this.state.session.target.bot_id,
                                        source,
                                        name,
                                    ),
                                )
                              : /^https?:\/\//i.test(local)
                                ? local
                                : '';
                        // 文件缓存可能跨过断连或账号切换，不能让旧任务进入新代次的发送队列。
                        requireCurrent();
                        if (!file) throw new Error('文件消息缺少可转发的来源');
                        operations.push({
                            action:
                                contact.type === 'group'
                                    ? 'upload_group_file'
                                    : 'upload_private_file',
                            params: {
                                [contact.type === 'group' ? 'group_id' : 'user_id']:
                                    this.state.session.peer(contact.key),
                                file,
                                name,
                                upload_file: true,
                            },
                            segments: [{ type: 'file', data: { name, file } }],
                        });
                    }
                    if (!operations.length) throw new Error('消息没有可转发的内容');
                    await this.queue.run(contact.key, operations, undefined, true, true);
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
                this.operations.finishSend(contact.key, epoch);
            }
            return;
        }
        const requestId = crypto.randomUUID();
        const peer = this.state.session.peer(contact.key);
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
            data: { id: this.state.session.protocolMessageId(message.id!) },
        }));
        this.operations.startSend(contact.key, epoch);
        const state = this.state.snapshot.account;
        this.state.account(
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
                response = await this.state.session.transport.call(
                    this.state.session.target.bot_id,
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
            if (!this.state.session.isCurrent(epoch, scope))
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
            this.state.account(
                settleSend(this.state.snapshot.account, requestId, {
                    state: 'sent',
                    id: id(record(result.data).message_id) || undefined,
                }),
            );
        } catch (error) {
            if (this.state.session.isCurrent(epoch, scope))
                this.state.account(
                    settleSend(this.state.snapshot.account, requestId, {
                        state:
                            error instanceof ChatForwardError && error.uncertain
                                ? 'unknown'
                                : 'failed',
                        error: errorText(error),
                    }),
                );
            throw error;
        } finally {
            this.operations.finishSend(contact.key, epoch);
        }
    }
}
