// 试聊页的连接和消息：页面挂着、麦麦在跑才连，切走就断。消息只在内存里，
// 每次连上上游补最近 50 条；昵称按实例记在本机。

import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import type { MaiBotChatEvent, MaiBotChatImage, MaiBotChatMessage } from '../../core/domain/apps/maibotChat';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import {
    maibotChatService,
    type MaiBotChatConnection,
    type MaiBotChatStatus,
} from '../../core/services/maibot-chat.service';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushAppErrorBar } from './pushAppErrorBar';
import { localFilesOrNothing } from './maibotResourceAction';

export type MaiBotChatItem =
    | { kind: 'message'; key: string; message: MaiBotChatMessage }
    | { kind: 'notice'; key: string; text: string; at: number; error: boolean };

export type MaiBotChatState = {
    status: MaiBotChatStatus;
    reason?: string;
    botName: string;
    botQq?: string;
    items: MaiBotChatItem[];
    typing: boolean;
    /** 给没 id 的消息、提示编 key */
    seq: number;
};

type Action =
    | { type: 'status'; status: MaiBotChatStatus; reason?: string }
    | { type: 'event'; event: MaiBotChatEvent }
    | { type: 'cleared' }
    | { type: 'reset' };

const INITIAL: MaiBotChatState = { status: 'connecting', botName: '麦麦', items: [], typing: false, seq: 0 };

export function chatReducer(s: MaiBotChatState, a: Action): MaiBotChatState {
    switch (a.type) {
        case 'reset':
            return INITIAL;
        case 'cleared':
            return { ...s, items: [] };
        case 'status':
            return { ...s, status: a.status, reason: a.reason, typing: a.status === 'ready' && s.typing };
        case 'event': {
            const e = a.event;
            switch (e.kind) {
                case 'session':
                    return { ...s, botName: e.botName, botQq: e.botQq };
                case 'history': {
                    // 重连时上游整段重发，直接换掉；提示条跟着没了也无妨
                    let seq = s.seq;
                    const items = e.messages.map((m): MaiBotChatItem => ({ kind: 'message', key: m.id || `h${++seq}`, message: m }));
                    return { ...s, items, seq };
                }
                case 'message': {
                    const id = e.message.id;
                    if (id && s.items.some((i) => i.kind === 'message' && i.message.id === id)) return s;
                    const seq = s.seq + 1;
                    return { ...s, seq, items: [...s.items, { kind: 'message', key: id || `m${seq}`, message: e.message }] };
                }
                case 'typing':
                    return { ...s, typing: e.typing };
                case 'notice': {
                    const seq = s.seq + 1;
                    return { ...s, seq, items: [...s.items, { kind: 'notice', key: `n${seq}`, text: e.text, at: e.at, error: e.error }] };
                }
                case 'nickname':
                    return s;
            }
        }
    }
}

export const DEFAULT_CHAT_NAME = '人类';
const nameKey = (instanceId: string) => `ncd.maibot.chat.name.${instanceId}`;

function readName(instanceId: string): string {
    try {
        return localStorage.getItem(nameKey(instanceId))?.trim() || DEFAULT_CHAT_NAME;
    } catch {
        return DEFAULT_CHAT_NAME;
    }
}

function writeName(instanceId: string, name: string) {
    try {
        localStorage.setItem(nameKey(instanceId), name);
    } catch {
        // 存不下就只在这次会话里生效
    }
}

export function useMaiBotChat(instanceId: string, live: boolean) {
    const [state, dispatch] = useReducer(chatReducer, INITIAL);
    const [userName, setUserName] = useState(() => readName(instanceId));
    const [clearing, setClearing] = useState(false);
    const conn = useRef<MaiBotChatConnection | null>(null);
    const nameRef = useRef(userName);
    nameRef.current = userName;

    useEffect(() => {
        if (!live) return;
        dispatch({ type: 'reset' });
        const c = maibotChatService.connect(instanceId, nameRef.current, {
            onEvent: (event) => dispatch({ type: 'event', event }),
            onStatus: (status, reason) => dispatch({ type: 'status', status, reason }),
        });
        conn.current = c;
        return () => {
            c.close();
            if (conn.current === c) conn.current = null;
        };
    }, [instanceId, live]);

    const send = useCallback(async (text: string, images: readonly MaiBotChatImage[]) => {
        const c = conn.current;
        if (!c) throw new Error('还没连上麦麦');
        await c.send(text, images);
    }, []);

    const rename = useCallback(
        (raw: string) => {
            const next = raw.trim().slice(0, 32) || DEFAULT_CHAT_NAME;
            setUserName(next);
            writeName(instanceId, next);
            void conn.current?.rename(next).catch(() => undefined);
        },
        [instanceId],
    );

    const clear = useCallback(async () => {
        setClearing(true);
        try {
            const done = await maibotChatService.clear(instanceId);
            dispatch({ type: 'cleared' });
            pushInfoBar({ key: `maibotChat:${instanceId}`, tone: 'success', title: done.message, autoDismissMs: 2500 });
        } catch (err) {
            pushAppErrorBar({ key: `maibotChat-fail:${instanceId}`, title: '没清掉', raw: toAppConfigError(err).message });
        } finally {
            setClearing(false);
        }
    }, [instanceId]);

    return { state, userName, send, rename, clear, clearing };
}

const chatImages = {
    pick: () => localFilesOrNothing(maibotChatService.pickImages(), 'maibotChat-images', '图片没读出来'),
    read: (paths: string[]) =>
        localFilesOrNothing(maibotChatService.localImages(paths), 'maibotChat-images', '图片没读出来'),
};

/** 输入框挑图、拖进窗口的图：读成能发的样子，预览就是要发出去的那份 */
export function useMaiBotChatImages() {
    return chatImages;
}
