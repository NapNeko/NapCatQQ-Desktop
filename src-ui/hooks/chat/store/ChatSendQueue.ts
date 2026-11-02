import { qqFaceService } from '../../../core/services/qq-face.service';
import { inlineImageService } from '../../../core/services/inline-image.service';
import { createChatMediaService } from '../../../core/services/chat-media.service';
import { qqFaceLarge } from '../../../core/domain/chat/qqFaces';
import { recoverDraft } from '../../../core/domain/chat/recoverDraft';
import {
    accountKey,
    addPending,
    settleSend,
    setDraft,
    record,
    id,
    text,
    EMPTY_DRAFT,
    type SessionKey,
} from '../../../core/domain/chat/model';
import { debugErrorCopy } from '../../../core/domain/debug/errorCopy';
import { errorText } from '../../../core/domain/errors';
import type { Segment } from '../../../core/domain/debug/segments';
import { ChatForwardError } from '../../../core/domain/chat/messageTransfer';
import type { ChatAccountState } from './ChatAccountState';
import type { ChatConversationActions } from './ChatConversationActions';
import type { ChatOperations } from './ChatOperations';

export type SendOperation = {
    action: string;
    params: unknown;
    segments: Segment[];
    resolveImages?: boolean;
    sourceMessageId?: string;
};

/** 发送管线：乐观消息、附件解析、进度、结果落账和失败后的草稿恢复。 */
export class ChatSendQueue {
    constructor(
        private state: ChatAccountState,
        private operations: ChatOperations,
        private conversations: ChatConversationActions,
    ) {}
    async run(
        key: SessionKey,
        operations: SendOperation[],
        retryKey?: string,
        preserveDraft = false,
        forwarding = false,
        operationLabel = '转发',
    ) {
        const epoch = this.state.session.epoch;
        const scope = accountKey(
            this.state.session.target.bot_id,
            String(this.state.session.target.qq_id),
        );
        const created: string[] = [];
        let completedOperations = 0;
        let prepared: SendOperation[];
        try {
            prepared = operations.map((operation) => ({
                ...operation,
                segments: this.state.media.externalizeSegments(operation.segments, created),
            }));
        } catch (error) {
            for (const reference of created)
                inlineImageService.discard(
                    this.state.session.target.bot_id,
                    String(this.state.session.target.qq_id),
                    reference,
                );
            throw error;
        }
        this.operations.startSend(key, epoch);
        if (!retryKey && !preserveDraft) this.conversations.draft(key, EMPTY_DRAFT);
        try {
            for (const operation of prepared) {
                const requestId = crypto.randomUUID();
                this.state.account(
                    retryKey
                        ? {
                              ...this.state.snapshot.account,
                              messages: this.state.snapshot.account.messages.map((message) =>
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
                              this.state.snapshot.account,
                              key,
                              requestId,
                              operation.segments,
                              Date.now(),
                          ),
                );
                if (!this.state.session.isCurrent(epoch, scope)) {
                    this.state.account(
                        settleSend(this.state.snapshot.account, requestId, {
                            state: 'failed',
                            error: '发送已中止，此条尚未提交',
                        }),
                    );
                    if (forwarding)
                        throw new ChatForwardError(`连接已改变，${operationLabel}已停止`, true);
                    continue;
                }
                if (
                    this.state.session.target.backend === 'snowluma' &&
                    operation.segments.some((segment) => segment.type === 'face')
                ) {
                    try {
                        await qqFaceService.validate(
                            this.state.session.target,
                            operation.segments
                                .filter((segment) => segment.type === 'face')
                                .map((segment) => text(segment.data.id)),
                            (...args) => this.state.session.transport.call(...args),
                        );
                    } catch (error) {
                        if (this.state.session.isCurrent(epoch, scope)) {
                            const failed = settleSend(this.state.snapshot.account, requestId, {
                                state: 'failed',
                                error: errorText(error),
                            });
                            const message = failed.messages.find(
                                (row) => row.requestId === requestId,
                            );
                            this.state.account(
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
                    if (!this.state.session.isCurrent(epoch, scope)) {
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
                            this.state.session.transport.call(...args),
                        );
                        const target = { ...this.state.session.target };
                        const segments: Segment[] = [];
                        let imageIndex = 0;
                        for (const segment of operation.segments) {
                            if (!this.state.session.isCurrent(epoch, scope)) break;
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
                        if (!this.state.session.isCurrent(epoch, scope)) {
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
                        this.state.session.target.backend === 'snowluma'
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
                            message: this.state.media.materializeSegments(
                                record(params).message as Segment[],
                            ),
                        };
                    submitted = true;
                    const response = await this.state.session.transport.call(
                        this.state.session.target.bot_id,
                        operation.action,
                        params,
                        requestId,
                        (progress) => {
                            if (
                                !this.state.session.isCurrent(epoch, scope) ||
                                progress.v !== 1 ||
                                progress.request_id !== requestId ||
                                !this.state.snapshot.account.messages.some(
                                    (message) =>
                                        message.requestId === requestId &&
                                        message.status === 'sending',
                                )
                            )
                                return;
                            this.state.update({
                                sendProgress: {
                                    ...this.state.snapshot.sendProgress,
                                    [requestId]: progress,
                                },
                            });
                        },
                    );
                    if (!this.state.session.isCurrent(epoch, scope)) {
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
                        this.state.account(
                            settleSend(this.state.snapshot.account, requestId, {
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
                            qqFaceService.invalidate(this.state.session.target);
                        this.state.account(
                            settleSend(this.state.snapshot.account, requestId, {
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
                    if (this.state.session.isCurrent(epoch, scope))
                        this.state.account(
                            settleSend(this.state.snapshot.account, requestId, {
                                state: submitted ? 'unknown' : 'failed',
                                error: errorText(error),
                            }),
                        );
                } finally {
                    if (
                        this.state.session.isCurrent(epoch, scope) &&
                        this.state.snapshot.sendProgress[requestId]
                    ) {
                        const next = { ...this.state.snapshot.sendProgress };
                        delete next[requestId];
                        this.state.update({ sendProgress: next });
                    }
                }
                if (forwarding) {
                    const message = this.state.snapshot.account.messages.find(
                        (row) => row.requestId === requestId,
                    );
                    if (!this.state.session.isCurrent(epoch, scope) || message?.status !== 'sent')
                        throw new ChatForwardError(
                            message?.error || `${operationLabel}结果待确认`,
                            completedOperations > 0 || message?.status !== 'failed',
                        );
                    completedOperations++;
                }
            }
        } finally {
            if (!forwarding) this.operations.finishSend(key, epoch);
        }
    }
}
