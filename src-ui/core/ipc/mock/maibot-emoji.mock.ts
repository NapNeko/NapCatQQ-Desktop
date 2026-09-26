// 浏览器预览：麦麦的表情包。语义照后端 resources/emoji：四种状态、按标签搜、三种排序；
// 收下 / 不再发 / 丢弃 / 捡回 / 删；图用 SVG 画一个带 emoji 的色块代替。

import { makeAppConfigError } from '../../domain/apps/appConfigError';
import { normalizeEmojiTags } from '../../domain/apps/maibotEmoji';
import type {
    AppInstance,
    MaiBotEmoji,
    MaiBotEmojiAction,
    MaiBotEmojiImage,
    MaiBotEmojiOverview,
    MaiBotEmojiPage,
    MaiBotEmojiQuery,
    MaiBotEmojiStatus,
    MaiBotEmojiUpload,
    MaiBotEmojiUploadDone,
    MaiBotLocalImage,
    MaiBotResourceDone,
} from '../types';
import { withMockDelay } from './bootstrap.mock';

const FACES: [string, string[]][] = [
    ['😂', ['开心', '大笑']],
    ['🥺', ['委屈', '可怜']],
    ['😡', ['生气']],
    ['😎', ['得意', '酷']],
    ['🤔', ['疑惑', '思考']],
    ['😭', ['哭哭', '难过']],
    ['🥰', ['喜欢', '比心']],
    ['😴', ['晚安', '困']],
    ['👍', ['点赞', '好的']],
    ['🙄', ['无语', '白眼']],
    ['😱', ['震惊']],
    ['🤡', ['搞笑', '小丑']],
    ['🐶', ['狗头']],
    ['🐱', ['猫猫', '可爱']],
    ['🌚', ['阴阳怪气']],
    ['🫠', ['融化', '摆烂']],
];
const COLORS = ['#FFE4D6', '#FDE2F3', '#E3F2FD', '#E8F5E9', '#FFF8E1', '#EDE7F6', '#FCE4EC', '#E0F7FA'];
const STATUS_CYCLE: MaiBotEmojiStatus[] = ['adopted', 'adopted', 'adopted', 'known', 'known', 'adopted', 'unknown', 'discarded'];

function svg(face: string, bg: string, px: number): string {
    const s = `<svg xmlns="http://www.w3.org/2000/svg" width="${px}" height="${px}" viewBox="0 0 100 100"><rect width="100" height="100" rx="16" fill="${bg}"/><text x="50" y="57" font-size="56" text-anchor="middle" dominant-baseline="middle">${face}</text></svg>`;
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(s)}`;
}

type Row = MaiBotEmoji & { face: string; color: string };
type Store = { rows: Row[]; nextId: number };
const stores = new Map<string, Store>();

function store(inst: AppInstance): Store {
    if (inst.state !== 'running') throw makeAppConfigError('not_running', '启动麦麦后才能用');
    let s = stores.get(inst.id);
    if (!s) {
        const now = Date.now() / 1000;
        const rows = Array.from({ length: 56 }, (_, i): Row => {
            const [face, tags] = FACES[i % FACES.length];
            const status = STATUS_CYCLE[i % STATUS_CYCLE.length];
            return {
                id: 500 + i,
                hash: `h${(i * 7919).toString(16)}`,
                format: i % 5 === 0 ? 'gif' : 'png',
                tags: status === 'unknown' ? [] : tags,
                status,
                usage_count: status === 'adopted' ? (i * 13) % 40 : 0,
                found_at: now - 3600 * (i * 3 + 1),
                adopted_at: status === 'adopted' ? now - 3600 * (i * 3) : undefined,
                last_used: status === 'adopted' && i % 3 !== 0 ? now - 600 * (i + 1) : undefined,
                face,
                color: COLORS[i % COLORS.length],
            };
        });
        s = { rows, nextId: 600 };
        stores.set(inst.id, s);
    }
    return s;
}

const strip = ({ face: _f, color: _c, ...e }: Row): MaiBotEmoji => e;
const done = (affected: number, message: string): Promise<MaiBotResourceDone> => withMockDelay({ affected, message });

function move(s: Store, ids: number[], to: (r: Row) => MaiBotEmojiStatus, verb: string): Promise<MaiBotResourceDone> {
    const now = Date.now() / 1000;
    s.rows = s.rows.map((r) => {
        if (!ids.includes(r.id)) return r;
        const status = to(r);
        return { ...r, status, adopted_at: status === 'adopted' ? now : r.adopted_at };
    });
    return done(ids.length, `${verb}了 ${ids.length} 张`);
}

const isImagePath = (p: string) => /\.(png|jpe?g|gif|webp)$/i.test(p);
const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;

export const mockMaiBotEmojis = {
    list(inst: AppInstance, q: MaiBotEmojiQuery): Promise<MaiBotEmojiPage> {
        const s = store(inst);
        const kw = q.search.trim();
        const rows = s.rows
            .filter((r) => q.filter === 'all' || r.status === q.filter)
            .filter((r) => !kw || r.tags.some((t) => t.includes(kw)) || r.hash.includes(kw))
            .sort((a, b) => {
                if (q.sort === 'most_used') return b.usage_count - a.usage_count;
                if (q.sort === 'recently_used') return (b.last_used ?? 0) - (a.last_used ?? 0);
                return (b.found_at ?? 0) - (a.found_at ?? 0);
            });
        const start = (Math.max(1, q.page) - 1) * q.page_size;
        return withMockDelay({ total: rows.length, items: rows.slice(start, start + q.page_size).map(strip) });
    },

    overview(inst: AppInstance): Promise<MaiBotEmojiOverview> {
        const s = store(inst);
        const n = (st: MaiBotEmojiStatus) => s.rows.filter((r) => r.status === st).length;
        return withMockDelay({ total: s.rows.length, adopted: n('adopted'), known: n('known'), unknown: n('unknown'), discarded: n('discarded') });
    },

    action(inst: AppInstance, a: MaiBotEmojiAction): Promise<MaiBotResourceDone> {
        const s = store(inst);
        switch (a.op) {
            case 'tag': {
                const tags = normalizeEmojiTags(a.tags);
                s.rows = s.rows.map((r) =>
                    r.id === a.id
                        ? { ...r, tags, status: r.status === 'known' || r.status === 'unknown' ? (tags.length ? 'known' : 'unknown') : r.status }
                        : r,
                );
                return done(1, '标签改好了');
            }
            case 'adopt':
                return move(s, a.ids, () => 'adopted', '收下');
            case 'unadopt':
                return move(s, a.ids, (r) => (r.tags.length ? 'known' : 'unknown'), '不再发');
            case 'discard':
                return move(s, a.ids, () => 'discarded', '丢弃');
            case 'restore':
                return move(s, a.ids, (r) => (r.tags.length ? 'known' : 'unknown'), '捡回');
            case 'delete':
                s.rows = s.rows.filter((r) => !a.ids.includes(r.id));
                return done(a.ids.length, a.ids.length > 1 ? `删掉了 ${a.ids.length} 张` : '删掉了');
        }
    },

    image(inst: AppInstance, id: number, original: boolean): Promise<MaiBotEmojiImage> {
        const r = store(inst).rows.find((x) => x.id === id);
        // 每 23 张缺一张图，演示上游清理过文件的样子
        if (!r || id % 23 === 0) return withMockDelay({});
        return withMockDelay({ data_url: svg(r.face, r.color, original ? 480 : 200) });
    },

    upload(inst: AppInstance, up: MaiBotEmojiUpload): Promise<MaiBotEmojiUploadDone> {
        const s = store(inst);
        const tags = normalizeEmojiTags(up.tags);
        const result: MaiBotEmojiUploadDone = { uploaded: 0, existed: 0, failed: [] };
        for (const p of up.paths) {
            if (!isImagePath(p)) {
                result.failed.push({ name: baseName(p), reason: '不是 PNG / JPG / GIF / WebP 图片' });
                continue;
            }
            const [face] = FACES[s.nextId % FACES.length];
            const now = Date.now() / 1000;
            s.rows.unshift({
                id: s.nextId++,
                hash: `u${s.nextId.toString(16)}`,
                format: p.split('.').pop()?.toLowerCase() ?? 'png',
                tags,
                status: 'adopted',
                usage_count: 0,
                found_at: now,
                adopted_at: now,
                face,
                color: COLORS[s.nextId % COLORS.length],
            });
            result.uploaded += 1;
        }
        return withMockDelay(result);
    },

    pickFiles(): Promise<string[]> {
        return withMockDelay(['C:/Users/me/Pictures/表情/开心.gif', 'C:/Users/me/Pictures/表情/无语.png', 'C:/Users/me/Pictures/表情/说明.txt']);
    },

    localImages(paths: string[]): Promise<MaiBotLocalImage[]> {
        return withMockDelay(
            paths.map((path, i) => {
                const [face] = FACES[(i * 5) % FACES.length];
                return isImagePath(path)
                    ? { path, name: baseName(path), size: 24_576 + i * 3_100, preview: svg(face, COLORS[i % COLORS.length], 160) }
                    : { path, name: baseName(path), size: 812, problem: '不是 PNG / JPG / GIF / WebP 图片' };
            }),
        );
    },
};
