// 试聊连接：先向后端要一张一次性票（WebUI 的会话 token 留在后端，远端实例给的是隧道口），
// 页面自己连麦麦的统一 WebSocket；握手后开会话、定时 ping，断了重新要票、带 restore 重连。
// 命令字面量只在此文件出现（R3）；浏览器预览走 mock。

import { APP_VERSION } from '../domain/app-meta';
import { chatFrames, parseChatFrame, type MaiBotChatEvent, type MaiBotChatImage } from '../domain/apps/maibotChat';
import { errorText } from '../domain/errors';
import { invoke, isTauri, pickImageFiles } from '../ipc/transport';
import type { MaiBotChatTicket, MaiBotLocalImage, MaiBotResourceDone } from '../ipc/types';
import { peekMockAppInstance } from '../ipc/mock/app-framework.mock';
import { mockMaiBotChat } from '../ipc/mock/maibot-chat.mock';
import { maibotResourcesService } from './maibot-resources.service';

/** connecting：还没开成过会话；reconnecting：开成过、断了在重连 */
export type MaiBotChatStatus = 'connecting' | 'ready' | 'reconnecting' | 'closed';

export type MaiBotChatHandlers = {
    onEvent: (event: MaiBotChatEvent) => void;
    /** reason 是上一次没连上 / 断开的原因 */
    onStatus: (status: MaiBotChatStatus, reason?: string) => void;
};

export interface MaiBotChatConnection {
    /** 上游收下就 resolve（回复另外以事件到） */
    send(text: string, images: readonly MaiBotChatImage[]): Promise<void>;
    rename(userName: string): Promise<void>;
    close(): void;
}

// 一条连接上只开一个会话；上游按「连接 id + 它」区分，名字随便取
const SESSION = 'desktop';
const PING_MS = 30_000;
// 30 秒一次心跳，两次半没动静就当连接死了（上游自己的页面等 90 秒）
const DEAD_MS = 75_000;
const CALL_TIMEOUT_MS = 10_000;
const RETRY_MAX_MS = 15_000;

type Pending = { resolve: () => void; reject: (e: Error) => void; timer: ReturnType<typeof setTimeout> };

class LiveChat implements MaiBotChatConnection {
    private ws: WebSocket | null = null;
    private userId = '';
    private closed = false;
    /** 开成过一次会话以后重连都带 restore：上游不再发欢迎语，历史照发 */
    private opened = false;
    private ready = false;
    private attempts = 0;
    private seq = 0;
    private lastSeen = 0;
    private pingTimer: ReturnType<typeof setInterval> | undefined;
    private retryTimer: ReturnType<typeof setTimeout> | undefined;
    private readonly pending = new Map<string, Pending>();

    constructor(
        private readonly instanceId: string,
        private userName: string,
        private readonly h: MaiBotChatHandlers,
    ) {
        h.onStatus('connecting');
        void this.connect();
    }

    private async connect() {
        let ticket: MaiBotChatTicket;
        try {
            ticket = await invoke<MaiBotChatTicket>('maibot_chat_ticket', { instanceId: this.instanceId });
        } catch (e) {
            this.retry(errorText(e));
            return;
        }
        if (this.closed) return;
        this.userId = ticket.user_id;
        const ws = new WebSocket(ticket.url);
        this.ws = ws;
        this.lastSeen = Date.now();
        ws.onmessage = (ev) => {
            if (this.ws === ws && typeof ev.data === 'string') this.onFrame(ev.data);
        };
        ws.onclose = (ev) => {
            // 4001 是上游握手时票不认（过期或用过了）
            if (this.ws === ws) this.drop(ev.code === 4001 ? '麦麦不认这次连接' : ev.reason || '连接断了');
        };
    }

    private onFrame(raw: string) {
        this.lastSeen = Date.now();
        const f = parseChatFrame(raw);
        if (!f) return;
        switch (f.op) {
            case 'ready':
                this.request((id) => chatFrames.open(id, SESSION, this.userId, this.userName, this.opened, APP_VERSION))
                    .then(() => {
                        this.opened = true;
                        this.ready = true;
                        this.attempts = 0;
                        this.startPing();
                        this.h.onStatus('ready');
                    })
                    .catch((e: Error) => this.drop(e.message));
                return;
            case 'response': {
                const p = this.pending.get(f.id);
                if (!p) return;
                this.pending.delete(f.id);
                clearTimeout(p.timer);
                if (f.ok) p.resolve();
                else p.reject(new Error(f.error || '麦麦没收下'));
                return;
            }
            case 'chat':
                if (f.session === SESSION) this.h.onEvent(f.event);
                return;
            case 'pong':
                return;
        }
    }

    private request(build: (id: string) => string): Promise<void> {
        const ws = this.ws;
        if (!ws || ws.readyState !== WebSocket.OPEN) return Promise.reject(new Error('还没连上麦麦'));
        const id = `c${++this.seq}`;
        return new Promise<void>((resolve, reject) => {
            const timer = setTimeout(() => {
                this.pending.delete(id);
                reject(new Error('麦麦没回应'));
            }, CALL_TIMEOUT_MS);
            this.pending.set(id, { resolve, reject, timer });
            ws.send(build(id));
        });
    }

    private startPing() {
        clearInterval(this.pingTimer);
        this.pingTimer = setInterval(() => {
            if (Date.now() - this.lastSeen > DEAD_MS) this.drop('麦麦好久没回应');
            else this.ws?.send(chatFrames.ping());
        }, PING_MS);
    }

    /** 这条连接作废：挂着的请求都失败，没关页面就过一会儿重连 */
    private drop(reason: string) {
        const ws = this.ws;
        this.ws = null;
        this.ready = false;
        clearInterval(this.pingTimer);
        for (const p of this.pending.values()) {
            clearTimeout(p.timer);
            p.reject(new Error(reason || '连接关了'));
        }
        this.pending.clear();
        if (ws && ws.readyState <= WebSocket.OPEN) ws.close();
        this.retry(reason);
    }

    private retry(reason: string) {
        if (this.closed) return;
        this.attempts += 1;
        const delay = Math.min(1000 * 2 ** (this.attempts - 1), RETRY_MAX_MS);
        this.h.onStatus(this.opened ? 'reconnecting' : 'connecting', reason);
        clearTimeout(this.retryTimer);
        this.retryTimer = setTimeout(() => void this.connect(), delay);
    }

    send(text: string, images: readonly MaiBotChatImage[]) {
        if (!this.ready) return Promise.reject(new Error('还没连上麦麦'));
        return this.request((id) => chatFrames.send(id, SESSION, this.userName, text, images));
    }

    async rename(userName: string) {
        this.userName = userName;
        if (this.ready) await this.request((id) => chatFrames.rename(id, SESSION, userName));
    }

    close() {
        if (this.closed) return;
        this.closed = true;
        clearTimeout(this.retryTimer);
        // 先告诉上游关会话；发不出去也无所谓，连接一断上游自己会收拾
        if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(chatFrames.close(`c${++this.seq}`, SESSION));
        this.drop('');
        this.h.onStatus('closed');
    }
}

export const maibotChatService = {
    connect(instanceId: string, userName: string, handlers: MaiBotChatHandlers): MaiBotChatConnection {
        if (!isTauri) return mockMaiBotChat.connect(peekMockAppInstance(instanceId), userName, handlers);
        return new LiveChat(instanceId, userName, handlers);
    },

    /** 清掉桌面端这段私聊的记录 */
    clear: async (instanceId: string): Promise<MaiBotResourceDone> => {
        if (!isTauri) return mockMaiBotChat.clear(peekMockAppInstance(instanceId));
        return invoke<MaiBotResourceDone>('maibot_chat_clear', { instanceId });
    },

    /** 系统对话框挑图，挑完读成能发的样子（预览就是要发的 data URL）；取消给空数组 */
    pickImages: async (): Promise<MaiBotLocalImage[]> => {
        if (!isTauri) return mockMaiBotChat.pickImages();
        const paths = await pickImageFiles('挑图片发给麦麦');
        return paths.length > 0 ? maibotResourcesService.localImages(paths) : [];
    },

    /** 拖进窗口的本机图片 */
    localImages: (paths: string[]): Promise<MaiBotLocalImage[]> => maibotResourcesService.localImages(paths),
};
