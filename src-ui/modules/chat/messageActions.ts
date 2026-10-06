import { EMPTY_DRAFT, type Draft, type Message } from '../../core/domain/chat/model';
import { mentionLabel, pruneMentions } from '../../core/domain/debug/composerModel';

/** 读取会话当前草稿并追加一条 @，同名成员按人数区分；消息菜单和头像菜单共用。 */
export function draftWithMention(
    messages: readonly Message[],
    drafts: Record<string, Draft>,
    sessionKey: string,
    message: Message,
): Draft {
    const sameName = new Set(
        messages
            .filter((item) => item.session === sessionKey && item.senderName === message.senderName)
            .map((item) => item.senderId),
    );
    sameName.add(message.senderId);
    return mentionMessageSender(drafts[sessionKey] ?? EMPTY_DRAFT, message, sameName.size);
}

/** Append an explicit mention without changing existing text or converting a typed @name. */
export function mentionMessageSender(draft: Draft, message: Message, sameNameCount = 1): Draft {
    const mentions = pruneMentions(draft.text, draft.mentions ?? []);
    const name = message.senderName.trim() || message.senderId;
    let label = mentionLabel(name, message.senderId, mentions, sameNameCount);
    const taken = (candidate: string) =>
        mentions.some(
            (mention) => mention.label === candidate && mention.qq !== message.senderId,
        ) ||
        (draft.text.includes(candidate) &&
            !mentions.some(
                (mention) => mention.label === candidate && mention.qq === message.senderId,
            ));
    if (taken(label)) label = `@${name}(${message.senderId})`;
    const base = label;
    for (let suffix = 2; taken(label); suffix++) label = `${base}[${suffix}]`;
    const separator = draft.text && !/\s$/.test(draft.text) ? ' ' : '';
    return {
        ...draft,
        text: `${draft.text}${separator}${label} `,
        mentions: mentions.some(
            (mention) => mention.qq === message.senderId && mention.label === label,
        )
            ? mentions
            : [...mentions, { qq: message.senderId, label }],
    };
}
