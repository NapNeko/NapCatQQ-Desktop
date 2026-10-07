import { describe, expect, it, vi } from 'vitest';
import { QQ_FACE_FALLBACK } from '../domain/chat/qqFaces';
import {
    createQQFaceService,
    parseAccountFaceCatalog,
    parseQQFaceCatalog,
    QQ_CLASSIC_FACES,
} from './qq-face.service';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import type { DebugCallResponse } from '../ipc/generated/debug/DebugCallResponse';

const face = (id: string, name = '/新表情') => ({
    emojiId: id,
    describe: name,
    assets: [{ type: 'png', path: `assets/qq_emoji/${id}/png/${id}.png` }],
});
describe('QQ face catalog', () => {
    it('accepts numeric image resources and skips other asset types without asserting send support', () => {
        expect(
            parseQQFaceCatalog({
                emojis: {
                    '600': face('600'),
                    '14': face('14', '/微笑'),
                    '700': { ...face('700'), removed: true },
                    '800': {
                        ...face('800'),
                        assets: [{ type: 'png', path: 'https://untrusted.test/image' }],
                    },
                    '😄': face('😄'),
                    '42': face('99'),
                },
            }),
        ).toEqual([
            { id: '14', name: '微笑' },
            { id: '600', name: '新表情' },
        ]);
    });
    it('shares catalog refreshes and caches successful updates', async () => {
        const fetcher = vi
            .fn()
            .mockResolvedValue(new Response(JSON.stringify({ emojis: { '600': face('600') } })));
        const service = createQQFaceService(fetcher);
        const [first, second] = await Promise.all([service.catalog(), service.catalog()]);
        expect(first).toEqual([{ id: '600', name: '新表情' }]);
        expect(second).toBe(first);
        expect(await service.catalog()).toBe(first);
        expect(fetcher).toHaveBeenCalledOnce();
    });
    it('keeps the complete fallback available offline and retries after the failure backoff', async () => {
        const now = Date.now();
        const time = vi.spyOn(Date, 'now').mockReturnValue(now);
        const fetcher = vi
            .fn()
            .mockRejectedValueOnce(new Error('offline'))
            .mockRejectedValueOnce(new Error('mirror offline'))
            .mockResolvedValueOnce(
                new Response(JSON.stringify({ emojis: { '600': face('600') } })),
            );
        const service = createQQFaceService(fetcher);
        expect(await service.catalog()).toBe(QQ_FACE_FALLBACK);
        expect(QQ_FACE_FALLBACK.length).toBeGreaterThan(300);
        expect(QQ_FACE_FALLBACK.some((face) => face.id === '507')).toBe(true);
        await service.catalog();
        expect(fetcher).toHaveBeenCalledTimes(2);
        time.mockReturnValue(now + 61_000);
        expect(await service.catalog()).toEqual([{ id: '600', name: '新表情' }]);
    });
    it('persists metadata categories and reloads them without a network round trip', async () => {
        const saved = new Map<string, string>();
        const store = {
            getItem: (key: string) => saved.get(key) ?? null,
            setItem: (key: string, value: string) => {
                saved.set(key, value);
            },
        };
        const fetcher = vi.fn().mockResolvedValue(
            new Response(
                JSON.stringify({
                    emojis: {
                        '364': {
                            ...face('364', '/超级赞'),
                            emojiType: 1,
                            assets: [
                                ...face('364').assets,
                                { type: 'apng', path: 'assets/qq_emoji/364/apng/364.png' },
                            ],
                        },
                    },
                }),
            ),
        );
        const service = createQQFaceService(fetcher, undefined, store);
        expect(await service.catalog()).toEqual([
            expect.objectContaining({
                id: '364',
                super: true,
                category: '超级',
                animationUrl: 'https://koishi.js.org/QFace/assets/qq_emoji/364/apng/364.png',
            }),
        ]);
        const offline = vi.fn().mockRejectedValue(new Error('offline'));
        expect(await createQQFaceService(offline, undefined, store).catalog()).toEqual(
            await service.catalog(),
        );
        expect(offline).not.toHaveBeenCalled();
    });
    it('uses the QFace CDN mirror when the primary directory fails', async () => {
        const fetcher = vi
            .fn()
            .mockRejectedValueOnce(new Error('unreachable'))
            .mockResolvedValueOnce(new Response(JSON.stringify({ emojis: { '14': face('14') } })));
        expect(await createQQFaceService(fetcher).catalog()).toEqual([
            { id: '14', name: '新表情' },
        ]);
        expect(fetcher.mock.calls[1][0]).toContain('cdn.jsdelivr.net/gh/koishijs/QFace');
    });
    it('projects corrupt optional fields out of a saved catalog', async () => {
        const saved = JSON.stringify({
            expires: Date.now() + 60_000,
            faces: [
                {
                    id: '14',
                    name: '微笑',
                    aliases: {},
                    category: { label: '经典' },
                    super: 'true',
                    url: 'javascript:alert(1)',
                },
            ],
        });
        const service = createQQFaceService(vi.fn(), undefined, {
            getItem: () => saved,
            setItem: vi.fn(),
        });
        expect(await service.catalog()).toEqual([{ id: '14', name: '微笑' }]);
    });
});

const target: DebugTarget = {
    bot_id: 'faces',
    qq_id: 99,
    name: '测试',
    backend: 'snowluma',
    running: true,
    online: true,
    host: { kind: 'local' },
};
const reply = (data: unknown): DebugCallResponse => ({
    request_id: 'req',
    result: {
        kind: 'ok',
        outcome: {
            ok: true,
            status: 'ok',
            retcode: 0,
            data,
            raw: {},
            truncated: false,
            size_bytes: 0,
            channel: { kind: 'internal' },
            elapsed_ms: 1,
            message: '',
            wording: '',
        },
    },
});
const packs = (...ids: string[]) => ({
    packs: [{ emojis: ids.map((id) => ({ q_sid: id, q_des: '/表情 ' + id, is_super: false })) }],
});
describe('account face catalog', () => {
    it('excludes Unicode IDs and incomplete super faces and deduplicates pack overlaps', () => {
        const value = {
            packs: [
                {
                    emojis: [
                        { q_sid: '14', q_des: '/微笑', emoji_name_alias: ['笑脸'] },
                        { q_sid: '😀', q_des: 'Unicode' },
                        {
                            q_sid: '486',
                            q_des: '残缺映射',
                            is_super: true,
                            ani_sticker_pack_id: null,
                        },
                        {
                            q_sid: '474',
                            q_des: '给你一拳',
                            is_super: true,
                            ani_sticker_pack_id: 1,
                            ani_sticker_id: 2,
                            ani_sticker_type: 3,
                        },
                    ],
                },
                { emojis: [{ q_sid: '14', q_des: '重复' }] },
            ],
        };
        expect(parseAccountFaceCatalog(value).map((face) => face.id)).toEqual(['14', '474']);
        expect(parseAccountFaceCatalog(value)[0]).toMatchObject({
            name: '微笑',
            aliases: ['笑脸'],
        });
    });
    it('refreshes the protocol catalog once and isolates supported IDs by account', async () => {
        const fetcher = vi.fn();
        const call = vi
            .fn()
            .mockResolvedValueOnce(reply(packs('14', '474')))
            .mockResolvedValueOnce(reply(packs('486')));
        const service = createQQFaceService(fetcher, call);
        const [first, duplicate] = await Promise.all([
            service.forAccount(target),
            service.forAccount(target),
        ]);
        expect(first).toBe(duplicate);
        expect(first.faces.map((face) => face.id)).toEqual(['14', '474']);
        expect(call).toHaveBeenCalledOnce();
        expect(call).toHaveBeenCalledWith('faces', 'fetch_sys_faces', { refresh: true });
        await expect(service.validate(target, ['486'])).rejects.toThrow('不支持 QQ 表情 486');
        const other = { ...target, qq_id: 100 };
        await expect(service.validate(other, ['486'])).resolves.toBeUndefined();
        expect((await service.forAccount(target)).faces.map((face) => face.id)).toEqual([
            '14',
            '474',
        ]);
        expect(fetcher).not.toHaveBeenCalled();
    });
    it('keeps the last authoritative catalog on refresh failure and falls back to classic faces for older servers', async () => {
        const call = vi
            .fn()
            .mockResolvedValueOnce(reply(packs('14', '474')))
            .mockRejectedValue(new Error('offline'));
        const service = createQQFaceService(vi.fn(), call);
        const first = await service.forAccount(target);
        expect(await service.forAccount(target, true)).toBe(first);
        const older = await service.forAccount({ ...target, qq_id: 101 });
        expect(older).toEqual({ faces: QQ_CLASSIC_FACES, limited: true });
        expect(older.faces.every((face) => Number(face.id) < 260)).toBe(true);
        await expect(service.validate({ ...target, qq_id: 101 }, ['486'])).rejects.toThrow(
            '不支持 QQ 表情 486',
        );
    });
});
