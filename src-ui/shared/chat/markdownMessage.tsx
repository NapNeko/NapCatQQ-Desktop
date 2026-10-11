// QQ Markdown 与内联键盘共用宿主链接、草稿和发送动作。
import { useId, useMemo, useRef, useState, useSyncExternalStore, type CSSProperties } from 'react';
import { ArrowUpRight, CornerDownLeft, LoaderCircle } from 'lucide-react';
import {
    markdownContent,
    markdownKeyboard,
    type MarkdownKeyboardButton,
} from '../../core/domain/debug/markdown';
import { ChatForwardError } from '../../core/domain/chat/messageTransfer';
import { errorText } from '../../core/domain/errors';
import { useChatNotice } from '../../hooks/chat/useChatNotice';
import { useMotion } from '../../hooks/preferences/useMotion';
import { SimpleMarkdown } from '../ui/SimpleMarkdown';
import { useChatView } from './chatContext';
import { LruCache } from './boundedCache';
import './markdownMessage.css';

// 虚拟列表会卸载消息行，未知发送结果不能因此重新变成可点击。
const buttonResults = new LruCache<'pending' | 'visited' | 'uncertain'>(512);
const resultListeners = new Set<() => void>();
let resultRevision = 0;
const subscribeResults = (listener: () => void) => {
    resultListeners.add(listener);
    return () => {
        resultListeners.delete(listener);
    };
};
const resultsSnapshot = () => resultRevision;
const publishResult = (key: string, result: 'pending' | 'visited' | 'uncertain' | null) => {
    if (result) buttonResults.set(key, result);
    else buttonResults.delete(key);
    resultRevision++;
    resultListeners.forEach((listener) => listener());
};

export function MarkdownMessage({
    data,
    messageId,
}: {
    data: Record<string, unknown>;
    messageId?: string;
}) {
    const { openLink, openImage } = useChatView();
    const hasKeyboard = useMemo(() => markdownKeyboard(data) !== null, [data]);
    return (
        <div className="my-0.5 block">
            <SimpleMarkdown
                text={markdownContent(data)}
                emptyFallback="（空的 Markdown）"
                onOpenLink={openLink}
                onOpenImage={openImage}
                className="text-[13px] leading-5 text-text"
            />
            {hasKeyboard && <MarkdownKeyboardView data={data} messageId={messageId} />}
        </div>
    );
}

function permissionReason(button: MarkdownKeyboardButton, selfId: number | undefined): string {
    if (button.permissionType === 2) return '';
    if (button.permissionType === 0)
        return selfId !== undefined && button.permittedUsers.includes(String(selfId))
            ? ''
            : '此按钮仅允许指定用户操作';
    if (button.permissionType === 1) return '此按钮需要 QQ 管理员权限';
    if (button.permissionType === 3) return '此按钮需要指定 QQ 身份组权限';
    return '按钮未提供可确认的操作权限';
}

export function MarkdownKeyboardView({
    data,
    messageId,
}: {
    data: Record<string, unknown>;
    messageId?: string;
}) {
    const keyboard = useMemo(() => markdownKeyboard(data), [data]);
    useSyncExternalStore(subscribeResults, resultsSnapshot, resultsSnapshot);
    const chat = useChatView();
    const motion = useMotion();
    const noticeId = useId();
    const inFlight = useRef(false);
    const [busy, setBusy] = useState<string | null>(null);
    const [error, setError] = useState('');
    useChatNotice(
        `${chat.mediaScope ?? ''}:keyboard:${messageId ?? noticeId}`,
        '按钮操作失败',
        error,
    );

    if (!keyboard) return <span className="text-xs text-text-tertiary">按钮内容不可用</span>;

    const stateKey = (key: string): string =>
        JSON.stringify([
            chat.mediaScope ?? `${chat.bot()?.id ?? ''}:${chat.selfId() ?? ''}`,
            messageId ?? noticeId,
            keyboard.botAppid,
            key,
        ]);
    const reasonFor = (button: MarkdownKeyboardButton, key: string): string => {
        if (buttonResults.get(stateKey(key)) === 'uncertain') return '结果待确认，请先检查会话';
        if (buttonResults.get(stateKey(key)) === 'pending') return '正在处理此按钮';
        const permission = permissionReason(button, chat.selfId());
        if (permission) return permission;
        switch (button.action.kind) {
            case 'unsupported':
                return button.action.reason;
            case 'callback':
                return '当前连接未提供 QQ 回调按钮能力';
            case 'command':
                if (!chat.onMarkdownCommand) return '当前页面不能执行消息指令';
                if (button.action.reply && !messageId) return '缺少原消息，无法发送引用指令';
                return '';
            default:
                return '';
        }
    };
    const activate = async (button: MarkdownKeyboardButton, key: string) => {
        if (inFlight.current || reasonFor(button, key)) return;
        const action = button.action;
        if (action.kind === 'link') {
            chat.openLink(action.url);
            publishResult(stateKey(key), 'visited');
            return;
        }
        if (action.kind !== 'command' || !chat.onMarkdownCommand) return;
        inFlight.current = true;
        publishResult(stateKey(key), 'pending');
        setBusy(key);
        setError('');
        try {
            await chat.onMarkdownCommand({
                text: action.text,
                enter: action.enter,
                reply: action.reply,
                messageId,
                botAppid: keyboard.botAppid,
            });
            publishResult(stateKey(key), 'visited');
        } catch (failure) {
            if (failure instanceof ChatForwardError && failure.uncertain) {
                publishResult(stateKey(key), 'uncertain');
            } else publishResult(stateKey(key), null);
            setError(errorText(failure));
        } finally {
            inFlight.current = false;
            setBusy(null);
        }
    };
    return (
        <div
            className="native-chat-markdown-keyboard"
            role="group"
            aria-label="消息按钮"
            aria-busy={busy !== null}
            data-motion={motion.enabled}
            style={{ '--keyboard-duration': `${motion.duration('fast')}s` } as CSSProperties}
        >
            {keyboard.rows.map((row, rowIndex) => (
                <div key={rowIndex} className="native-chat-markdown-keyboard-row">
                    {row.map((button, buttonIndex) => {
                        const key = `${rowIndex}:${buttonIndex}:${button.id}`;
                        const reason = reasonFor(button, key);
                        const action = button.action;
                        const hint =
                            reason ||
                            (action.kind === 'command'
                                ? `${action.enter ? '发送' : '填入输入框'}：${action.text}`
                                : action.kind === 'link'
                                  ? action.url
                                  : '');
                        const label =
                            buttonResults.get(stateKey(key)) === 'visited' && button.visitedLabel
                                ? button.visitedLabel
                                : button.label;
                        return (
                            <button
                                key={key}
                                type="button"
                                className="native-chat-markdown-button"
                                data-primary={button.primary}
                                data-busy={busy === key}
                                disabled={busy !== null || !!reason}
                                title={hint || label}
                                onClick={(event) => {
                                    event.stopPropagation();
                                    void activate(button, key);
                                }}
                            >
                                <span>{label}</span>
                                {busy === key ? (
                                    <LoaderCircle size={12} aria-hidden />
                                ) : action.kind === 'link' ? (
                                    <ArrowUpRight size={12} aria-hidden />
                                ) : action.kind === 'command' ? (
                                    <CornerDownLeft size={12} aria-hidden />
                                ) : null}
                            </button>
                        );
                    })}
                </div>
            ))}
        </div>
    );
}
