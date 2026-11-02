// 视图只订阅自己消费的数据；草稿和上传进度不再让整个聊天工作区刷新。
import type { ChatSnapshot } from '../../hooks/chat/chatStore';
import type { Message, SessionKey } from '../../core/domain/chat/model';

function sameFields<T extends object>(left: T, right: T): boolean {
    const keys = Object.keys(left) as (keyof T)[];
    return (
        keys.length === Object.keys(right).length &&
        keys.every((key) => Object.is(left[key], right[key]))
    );
}

export function selectWorkspace(snapshot: ChatSnapshot) {
    const { active, conversations, messages, archiveMessages } = snapshot.account;
    return {
        account: { active, conversations, messages, archiveMessages },
        contacts: snapshot.contacts,
        connection: snapshot.connection,
        error: snapshot.error,
        contactsLoading: snapshot.contactsLoading,
        hydrated: snapshot.hydrated,
        archiveError: snapshot.archiveError,
        recentLoading: snapshot.recentLoading,
        recentError: snapshot.recentError,
        history: snapshot.history,
    };
}

export function equalWorkspace(
    left: ReturnType<typeof selectWorkspace>,
    right: ReturnType<typeof selectWorkspace>,
) {
    return (
        sameFields(left.account, right.account) &&
        sameFields({ ...left, account: right.account }, right)
    );
}

// 只保留这个 selector 的上次投影，账号内其他会话变更时沿用当前列表引用。
export function createConversationMessagesSelector(session: SessionKey | null) {
    let source: Message[] | undefined;
    let messages: Message[] = [];
    return (snapshot: ChatSnapshot) => {
        if (source !== snapshot.account.messages) {
            source = snapshot.account.messages;
            const next = source.filter((message) => message.session === session);
            if (
                next.length !== messages.length ||
                next.some((message, index) => message !== messages[index])
            )
                messages = next;
        }
        return messages;
    };
}

export function createTimelineSelector(session: SessionKey, messages: Message[]) {
    const requests = messages.flatMap((message) => (message.requestId ? [message.requestId] : []));
    // @ 显示仍可引用其他会话获知的昵称，但只订阅当前消息实际使用的名字。
    const mentioned = new Set(
        messages.flatMap((message) =>
            message.segments.flatMap((segment) =>
                segment.type === 'at' && Number.isFinite(Number(segment.data.qq))
                    ? [String(Number(segment.data.qq))]
                    : [],
            ),
        ),
    );
    // 转发内容按需读取，未知的嵌套 @ 仍需跟随账号昵称变化。
    const containsForward = messages.some((message) =>
        message.segments.some((segment) => segment.type === 'forward'),
    );
    let nameSource: Message[] | undefined;
    let names: Record<string, string> = {};
    return (snapshot: ChatSnapshot) => {
        if (nameSource !== snapshot.account.messages) {
            nameSource = snapshot.account.messages;
            const next: Record<string, string> = {};
            if (containsForward || mentioned.size)
                for (const message of nameSource) {
                    if (containsForward || mentioned.has(message.senderId))
                        next[message.senderId] = message.senderName;
                }
            if (!sameFields(names, next)) names = next;
        }
        const sendProgress: ChatSnapshot['sendProgress'] = {};
        for (const id of requests) {
            const progress = snapshot.sendProgress[id];
            if (progress) sendProgress[id] = progress;
        }
        return {
            connection: snapshot.connection.state,
            history: snapshot.history[session],
            sendProgress,
            names,
        };
    };
}

export function equalTimeline(
    left: ReturnType<ReturnType<typeof createTimelineSelector>>,
    right: ReturnType<ReturnType<typeof createTimelineSelector>>,
) {
    return (
        left.connection === right.connection &&
        left.history === right.history &&
        left.names === right.names &&
        sameFields(left.sendProgress, right.sendProgress)
    );
}

export function createSendingSelector(session: SessionKey) {
    let messages: Message[] | undefined;
    let sending = false;
    return (snapshot: ChatSnapshot) => {
        if (messages !== snapshot.account.messages) {
            messages = snapshot.account.messages;
            sending = messages.some(
                (message) => message.session === session && message.status === 'sending',
            );
        }
        return sending;
    };
}
