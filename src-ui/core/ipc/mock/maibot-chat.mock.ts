// 浏览器预览：试聊。连上先给会话信息、最近的记录和一次欢迎语；发一句先回显、打字中，
// 过一会儿回一句（发了图的回图，问句回得犹豫些）。记录按实例留在内存里，清空后重来。

import type { MaiBotChatImage, MaiBotChatMessage, MaiBotChatSegment } from '../../domain/apps/maibotChat';
import type { MaiBotChatConnection, MaiBotChatHandlers } from '../../services/maibot-chat.service';
import type { AppInstance, MaiBotLocalImage, MaiBotResourceDone } from '../types';
import { withMockDelay } from './bootstrap.mock';

const SMILE_SVG =
    "<svg xmlns='http://www.w3.org/2000/svg' width='128' height='128' viewBox='0 0 64 64'><circle cx='32' cy='32' r='30' fill='#ffd166'/>" +
    "<circle cx='22' cy='26' r='4' fill='#5b3a29'/><circle cx='42' cy='26' r='4' fill='#5b3a29'/>" +
    "<path d='M18 38 Q32 52 46 38' stroke='#5b3a29' stroke-width='4' fill='none' stroke-linecap='round'/></svg>";
const SMILE = `data:image/svg+xml;base64,${btoa(SMILE_SVG)}`;

const REPLIES = ['哈哈哈哈', '真的假的', '我也这么觉得', '嗯嗯，然后呢', '好耶', '你说得对'];

const histories = new Map<string, MaiBotChatMessage[]>();
const welcomed = new Set<string>();

function seed(): MaiBotChatMessage[] {
    const now = Date.now() / 1000;
    const text = (t: string): MaiBotChatSegment[] => [{ type: 'text', text: t }];
    return [
        { id: 'mock-h1', fromBot: false, senderName: '人类', segments: text('麦麦早上好'), at: now - 86400 - 3600 },
        { id: 'mock-h2', fromBot: true, senderName: '麦麦', segments: text('早呀～今天也要元气满满'), at: now - 86400 - 3590 },
        { id: 'mock-h3', fromBot: false, senderName: '人类', segments: text('今天考试好紧张'), at: now - 1800 },
        {
            id: 'mock-h4',
            fromBot: true,
            senderName: '麦麦',
            segments: [
                { type: 'reply', sender: '人类', text: '今天考试好紧张' },
                { type: 'text', text: '别怕，你复习得那么认真，肯定没问题的' },
                { type: 'emoji', src: SMILE },
            ],
            at: now - 1780,
        },
    ];
}

function replyTo(text: string, images: number, n: number): MaiBotChatSegment[] {
    if (images > 0) return [{ type: 'text', text: images > 1 ? `一下发了 ${images} 张！都好好看` : '这图好可爱！' }, { type: 'emoji', src: SMILE }];
    if (/[?？吗]$/.test(text.trim())) return [{ type: 'text', text: '这个嘛……我也说不准，你觉得呢' }];
    return [{ type: 'text', text: REPLIES[n % REPLIES.length] }];
}

export const mockMaiBotChat = {
    connect(inst: AppInstance, userName: string, h: MaiBotChatHandlers): MaiBotChatConnection {
        let name = userName;
        let closed = false;
        let n = 0;
        const timers = new Set<ReturnType<typeof setTimeout>>();
        const later = (ms: number, fn: () => void) => {
            const t = setTimeout(() => {
                timers.delete(t);
                if (!closed) fn();
            }, ms);
            timers.add(t);
        };
        const history = histories.get(inst.id) ?? seed();
        histories.set(inst.id, history);

        h.onStatus('connecting');
        later(500, () => {
            if (inst.state !== 'running') {
                h.onStatus('connecting', '麦麦没在跑');
                return;
            }
            h.onStatus('ready');
            h.onEvent({ kind: 'session', botName: '麦麦', userName: name });
            h.onEvent({ kind: 'history', messages: [...history] });
            if (!welcomed.has(inst.id)) {
                welcomed.add(inst.id);
                h.onEvent({ kind: 'notice', text: '已连接到本地聊天室，可以开始与 麦麦 对话了！', at: Date.now() / 1000, error: false });
            }
        });

        return {
            async send(text: string, images: readonly MaiBotChatImage[]) {
                const at = Date.now() / 1000;
                n += 1;
                const mine: MaiBotChatMessage = {
                    id: `mock-u${at}-${n}`,
                    fromBot: false,
                    senderName: name,
                    segments: [
                        ...(text ? [{ type: 'text' as const, text }] : []),
                        ...images.map((i) => ({ type: 'image' as const, src: `data:${i.mimeType};base64,${i.base64}` })),
                    ],
                    at,
                };
                history.push(mine);
                const turn = n;
                later(60, () => h.onEvent({ kind: 'message', message: mine }));
                later(350, () => h.onEvent({ kind: 'typing', typing: true }));
                later(1600, () => {
                    const reply: MaiBotChatMessage = {
                        id: '',
                        fromBot: true,
                        senderName: '麦麦',
                        segments: replyTo(text, images.length, turn),
                        at: Date.now() / 1000,
                    };
                    history.push(reply);
                    h.onEvent({ kind: 'message', message: reply });
                    h.onEvent({ kind: 'typing', typing: false });
                });
            },
            async rename(next: string) {
                name = next;
                later(100, () => h.onEvent({ kind: 'nickname', userName: next }));
            },
            close() {
                closed = true;
                timers.forEach(clearTimeout);
                timers.clear();
                h.onStatus('closed');
            },
        };
    },

    clear(inst: AppInstance): Promise<MaiBotResourceDone> {
        const count = histories.get(inst.id)?.length ?? 0;
        histories.set(inst.id, []);
        return withMockDelay({ affected: count, message: `已清空 ${count} 条聊天记录` });
    },

    /** 系统对话框挑到的两张图；预览是 base64，和后端给的一样能直接发 */
    pickImages(): Promise<MaiBotLocalImage[]> {
        const cat =
            "<svg xmlns='http://www.w3.org/2000/svg' width='360' height='270' viewBox='0 0 120 90'><rect width='120' height='90' rx='12' fill='#f4a261'/>" +
            "<circle cx='45' cy='42' r='6' fill='#264653'/><circle cx='75' cy='42' r='6' fill='#264653'/>" +
            "<path d='M52 58 Q60 64 68 58' stroke='#264653' stroke-width='3' fill='none'/></svg>";
        return withMockDelay([
            { path: 'C:/Users/me/Pictures/橘猫.png', name: '橘猫.png', size: 48_120, preview: `data:image/svg+xml;base64,${btoa(cat)}` },
            { path: 'C:/Users/me/Pictures/笑脸.png', name: '笑脸.png', size: 12_004, preview: SMILE },
        ]);
    },
};
