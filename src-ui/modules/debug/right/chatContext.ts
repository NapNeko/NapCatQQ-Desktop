// 时间线里每一行都要用到的查询和操作：按 message_id 找被回复的消息、按 QQ 号找名字、打开详情、选中、回复、填入请求、处理加好友和加群请求……
//
// 放进 context 而不是一层层传 props：虚拟列表里的行是 memo 的，只有自己的条目变了才重画；
// 这里的值由右栏建一次、之后引用不变（函数内部读 ref 拿最新数据），所以聊天每帧刷新时不会带着所有行一起重画。

import { createContext, useContext } from 'react';
import type { ChatItem, SessionKey } from '../../../core/domain/debug/chat';
import type { FillPlan, MessageItem } from '../../../core/domain/debug/chatFormat';
import type { RequestItem } from '../../../core/domain/debug/requestHandling';
import type { ForwardNode } from '../../../core/services/chat-media.service';

/** 「同意 / 拒绝」的结果：失败给一句原因 */
export type RequestHandleResult = { ok: true } | { ok: false; reason: string };

export interface ChatViewApi {
    /** 按 message_id 找消息（在整条时间线里找，不受筛选影响）；找不到是 undefined */
    findMessage: (messageId: number) => MessageItem | undefined;
    /** QQ 号 → 最近一次见到的名字；自己的号给 Bot 的名字 */
    nameOf: (userId: number) => string | undefined;
    sessionName: (key: SessionKey) => string | undefined;
    selfId: () => number | undefined;
    /** 当前观察的 Bot：请求卡片的危险确认要 botId / 显示名 */
    bot: () => { id: string; name: string } | null;

    /** 点气泡：选中（再点一次取消），输入框据此回复它 / 发到它所在的会话 */
    toggleSelect: (item: MessageItem) => void;
    /** 悬停工具条的「回复」：只选中，不会取消 */
    reply: (item: MessageItem) => void;
    openDetail: (item: ChatItem, anchor: HTMLElement) => void;
    /** 预演「填入请求」：能填哪些、为什么不能填 */
    previewFill: (item: ChatItem) => FillPlan;
    /** 真的填进去；返回填了哪些参数 */
    fill: (item: ChatItem) => FillPlan;
    /** 请求卡片的「同意 / 拒绝」：走现有调用链发 set_*_add_request */
    handleRequest: (item: RequestItem, approve: boolean) => Promise<RequestHandleResult>;
    openImage: (url: string) => void;
    openLink: (url: string) => void;
    readImage?: (data: Record<string, unknown>, refresh?: boolean) => Promise<string>;
    readForward?: (data: Record<string, unknown>) => Promise<ForwardNode[]>;
    readRecord?: (data: Record<string, unknown>) => Promise<string>;
    readVideo?: (data: Record<string, unknown>, refresh?: boolean) => Promise<string>;
    readRecordText?: (messageId: string) => Promise<string>;
    /** 滚到被回复的那条消息并闪一下；它不在当前列表里时返回 false */
    revealMessage: (messageId: number) => boolean;

    /** 「展开」过的长消息：行被虚拟列表卸掉再挂上时还是展开的 */
    isExpanded: (key: string) => boolean;
    setExpanded: (key: string, expanded: boolean) => void;
}

const noop = () => {};
const none = () => undefined;
const cannot = (): FillPlan => ({ ok: false, reason: '当前没有请求标签' });

const FALLBACK: ChatViewApi = {
    findMessage: none,
    nameOf: none,
    sessionName: none,
    selfId: none,
    bot: () => null,
    toggleSelect: noop,
    reply: noop,
    openDetail: noop,
    previewFill: cannot,
    fill: cannot,
    handleRequest: async () => ({ ok: false, reason: '当前没有 Bot' }),
    openImage: noop,
    openLink: noop,
    revealMessage: () => false,
    isExpanded: () => false,
    setExpanded: noop,
};

export const ChatViewContext = createContext<ChatViewApi>(FALLBACK);

export function useChatView(): ChatViewApi {
    return useContext(ChatViewContext);
}
