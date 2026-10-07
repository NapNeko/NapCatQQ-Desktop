import type { Message } from './model';

export const FORWARD_MESSAGE_LIMIT = 20;

export function canForwardMessage(message: Message): boolean {
    return (
        message.status === 'sent' &&
        !!message.id &&
        /^-?\d+$/.test(message.id) &&
        !/^-?0+$/.test(message.id) &&
        !message.recalled &&
        !message.notice
    );
}

export function canRecallMessage(message: Message): boolean {
    return message.mine && canForwardMessage(message);
}

const repeatableSegments = new Set(['text', 'at', 'reply', 'face', 'image', 'mface', 'markdown']);
export function canRepeatMessage(message: Message): boolean {
    return (
        message.status === 'sent' &&
        !message.recalled &&
        !message.notice &&
        message.segments.some((segment) => segment.type !== 'reply') &&
        message.segments.every((segment) => repeatableSegments.has(segment.type))
    );
}

export class ChatForwardError extends Error {
    constructor(
        message: string,
        readonly uncertain = false,
    ) {
        super(message);
        this.name = 'ChatForwardError';
    }
}
