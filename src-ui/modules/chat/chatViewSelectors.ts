// 视图只订阅自己消费的数据；草稿和上传进度不再让整个聊天工作区刷新。
import type { ChatSnapshot } from '../../hooks/chat/chatStore';
import type { Message, SessionKey } from '../../core/domain/chat/model';

function sameFields<T extends object>(left: T, right: T): boolean {
    return (Object.keys(left) as (keyof T)[]).every((key) => Object.is(left[key], right[key]));
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

export function selectTimeline(snapshot: ChatSnapshot) {
    return {
        messages: snapshot.account.messages,
        connection: snapshot.connection,
        history: snapshot.history,
        sendProgress: snapshot.sendProgress,
    };
}

export const equalTimeline = sameFields<ReturnType<typeof selectTimeline>>;

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
