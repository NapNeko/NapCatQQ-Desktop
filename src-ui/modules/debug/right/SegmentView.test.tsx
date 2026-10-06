import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SegmentList } from './SegmentView';
import { ChatViewContext, useChatView, type ChatViewApi } from './chatContext';
function NativeSegments({
    api,
    segments,
    messageId,
}: {
    api: Partial<ChatViewApi>;
    segments: Parameters<typeof SegmentList>[0]['segments'];
    messageId?: string;
}) {
    return (
        <ChatViewContext.Provider value={{ ...useChatView(), ...api }}>
            <SegmentList mine={false} messageId={messageId} segments={segments} />
        </ChatViewContext.Provider>
    );
}
describe('native media segments', () => {
    it('does not reset a decoded image when history supplies an equivalent segment object', () => {
        const data = { file: '0', url: 'https://cdn.example/stable-layout.gif', sub_type: 1 };
        const view = render(
            <NativeSegments
                api={{ mediaScope: 'stable-layout' }}
                segments={[{ type: 'image', data }]}
            />,
        );
        const image = screen.getByAltText('图片');
        Object.defineProperties(image, {
            naturalWidth: { value: 400 },
            naturalHeight: { value: 100 },
        });
        fireEvent.load(image);
        const style = image.parentElement!.getAttribute('style');
        view.rerender(
            <NativeSegments
                api={{ mediaScope: 'stable-layout' }}
                segments={[{ type: 'image', data: { ...data } }]}
            />,
        );
        expect(screen.getByAltText('图片')).toBe(image);
        expect(image).toHaveClass('opacity-100', 'absolute');
        expect(image.parentElement!.getAttribute('style')).toBe(style);
    });
    it('retains decoded geometry after a refreshed URL expires and the row remounts', async () => {
        const now = Date.now();
        const time = vi.spyOn(Date, 'now').mockReturnValue(now);
        const data = { file: '0', url: 'https://cdn.example/layout-expired.gif', sub_type: 1 };
        const api = {
            mediaScope: 'layout-expiry',
            readImage: vi.fn().mockResolvedValue('https://cdn.example/layout-refreshed.gif'),
        };
        const first = render(<NativeSegments api={api} segments={[{ type: 'image', data }]} />);
        fireEvent.error(screen.getByAltText('图片'));
        await waitFor(() =>
            expect(screen.getByAltText('图片')).toHaveAttribute(
                'src',
                'https://cdn.example/layout-refreshed.gif',
            ),
        );
        Object.defineProperties(screen.getByAltText('图片'), {
            naturalWidth: { value: 400 },
            naturalHeight: { value: 100 },
        });
        fireEvent.load(screen.getByAltText('图片'));
        const style = screen.getByAltText('图片').parentElement!.getAttribute('style');
        first.unmount();
        time.mockReturnValue(now + 61_000);
        render(<NativeSegments api={api} segments={[{ type: 'image', data: { ...data } }]} />);
        expect(screen.getByAltText('图片')).toHaveAttribute('src', data.url);
        expect(screen.getByAltText('图片').parentElement!.getAttribute('style')).toBe(style);
    });
    it('keeps an in-flight read alive across equivalent history updates', async () => {
        let finish!: (value: string) => void;
        const readImage = vi.fn(
            () =>
                new Promise<string>((resolve) => {
                    finish = resolve;
                }),
        );
        const data = { file: 'opaque-equivalent-image' };
        const view = render(
            <NativeSegments api={{ readImage }} segments={[{ type: 'image', data }]} />,
        );
        view.rerender(
            <NativeSegments
                api={{ readImage }}
                segments={[{ type: 'image', data: { ...data } }]}
            />,
        );
        await act(async () => finish('https://cdn.example/equivalent-image.png'));
        expect(readImage).toHaveBeenCalledOnce();
        expect(screen.getByAltText('图片')).toHaveAttribute(
            'src',
            'https://cdn.example/equivalent-image.png',
        );
    });
    it('never substitutes a cached sticker from another URL sharing the same file name', () => {
        const first = {
            type: 'image',
            data: { file: '0', url: 'https://cdn.example/sticker-a.gif', sub_type: 1 },
        };
        const view = render(
            <NativeSegments api={{ mediaScope: 'sticker-selection' }} segments={[first]} />,
        );
        const old = screen.getByAltText('图片');
        Object.defineProperties(old, {
            naturalWidth: { value: 128 },
            naturalHeight: { value: 128 },
        });
        fireEvent.load(old);
        view.rerender(
            <NativeSegments
                api={{ mediaScope: 'sticker-selection' }}
                segments={[
                    { ...first, data: { ...first.data, url: 'https://cdn.example/sticker-b.gif' } },
                ]}
            />,
        );
        expect(screen.getByAltText('图片')).not.toBe(old);
        expect(screen.getByAltText('图片')).toHaveAttribute(
            'src',
            'https://cdn.example/sticker-b.gif',
        );
        fireEvent.load(old);
        expect(screen.getByAltText('图片')).toHaveAttribute(
            'src',
            'https://cdn.example/sticker-b.gif',
        );
    });
    it('does not inherit dimensions from a different favorite with the placeholder file name 0', () => {
        const view = render(
            <SegmentList
                mine={false}
                segments={[
                    {
                        type: 'image',
                        data: {
                            file: '0',
                            url: 'https://cdn.example/wide-favorite.gif',
                            sub_type: 1,
                        },
                    },
                ]}
            />,
        );
        const first = screen.getByAltText('图片');
        Object.defineProperties(first, {
            naturalWidth: { value: 128 },
            naturalHeight: { value: 32 },
        });
        fireEvent.load(first);
        view.rerender(
            <SegmentList
                mine={false}
                segments={[
                    {
                        type: 'image',
                        data: {
                            file: '0',
                            url: 'https://cdn.example/tall-favorite.gif',
                            sub_type: 1,
                            width: 32,
                            height: 128,
                        },
                    },
                ]}
            />,
        );
        expect(screen.getByAltText('图片').parentElement).toHaveStyle({
            width: '32px',
            aspectRatio: '32 / 128',
        });
    });
    it('reserves protocol dimensions before a delayed image loads', () => {
        render(
            <SegmentList
                mine={false}
                segments={[
                    {
                        type: 'image',
                        data: {
                            url: 'https://cdn.example/known-delayed.png',
                            width: 1600,
                            height: 800,
                        },
                    },
                ]}
            />,
        );
        const image = screen.getByAltText('图片');
        const before = image.parentElement!.getAttribute('style');
        expect(image.parentElement).toHaveStyle({ width: '320px', aspectRatio: '320 / 160' });
        Object.defineProperties(image, {
            naturalWidth: { value: 1600 },
            naturalHeight: { value: 800 },
        });
        fireEvent.load(image);
        expect(image.parentElement!.getAttribute('style')).toBe(before);
        expect(image).toHaveAttribute('loading', 'eager');
    });
    it('reuses image dimensions when a signed URL changes for the same file', () => {
        const view = render(
            <SegmentList
                mine={false}
                segments={[
                    {
                        type: 'image',
                        data: {
                            url: 'https://cdn.example/signed.png?old',
                            file_id: 'signed-file-test',
                        },
                    },
                ]}
            />,
        );
        const image = screen.getByAltText('图片');
        Object.defineProperties(image, {
            naturalWidth: { value: 1200 },
            naturalHeight: { value: 600 },
        });
        fireEvent.load(image);
        view.rerender(
            <SegmentList
                mine={false}
                segments={[
                    {
                        type: 'image',
                        data: {
                            url: 'https://cdn.example/signed.png?new',
                            file_id: 'signed-file-test',
                        },
                    },
                ]}
            />,
        );
        expect(screen.getByAltText('图片').parentElement).toHaveStyle({
            width: '320px',
            aspectRatio: '320 / 160',
        });
    });
    it('recognizes animated stickers sent as an image segment', () => {
        render(
            <SegmentList
                mine={false}
                segments={[
                    {
                        type: 'image',
                        data: { url: 'https://cdn.example/sticker.gif', sub_type: 1 },
                    },
                ]}
            />,
        );
        const img = screen.getByAltText('图片');
        Object.defineProperties(img, {
            naturalWidth: { value: 400 },
            naturalHeight: { value: 200 },
        });
        fireEvent.load(img);
        expect(img.parentElement).toHaveStyle({ width: '128px', aspectRatio: '128 / 64' });
    });
    it.each([
        ['wide', 'image', 1000, 300, 320, 96],
        ['tall', 'image', 300, 1000, 84, 280],
        ['small', 'image', 40, 20, 40, 20],
        ['animated', 'mface', 400, 200, 128, 64],
    ])(
        'preserves the complete %s image aspect ratio',
        (name, type, width, height, expectedWidth, expectedHeight) => {
            render(
                <SegmentList
                    mine={false}
                    segments={[
                        { type: String(type), data: { url: `https://cdn.example/${name}.gif` } },
                    ]}
                />,
            );
            const img = screen.getByAltText(type === 'mface' ? '[表情包]' : '图片');
            Object.defineProperties(img, {
                naturalWidth: { value: width },
                naturalHeight: { value: height },
            });
            fireEvent.load(img);
            expect(img).toHaveClass('object-contain');
            expect(img.parentElement).toHaveStyle({
                width: `${expectedWidth}px`,
                aspectRatio: `${expectedWidth} / ${expectedHeight}`,
                maxWidth: '100%',
            });
        },
    );
    it('opens fetched forwarded nodes and lets nested forwards be read safely', async () => {
        const readForward = vi
            .fn()
            .mockResolvedValueOnce([
                {
                    senderId: '123',
                    name: '小明',
                    segments: [
                        { type: 'text', data: { text: '<script>unsafe</script>' } },
                        { type: 'forward', data: { id: 'nested' } },
                    ],
                },
            ])
            .mockResolvedValueOnce([
                {
                    senderId: '124',
                    name: '小李',
                    segments: [{ type: 'text', data: { text: '内层内容' } }],
                },
            ]);
        render(
            <NativeSegments
                api={{ readForward }}
                segments={[{ type: 'forward', data: { id: 'outer' } }]}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '查看聊天记录' }));
        expect(await screen.findByText('<script>unsafe</script>')).toBeInTheDocument();
        expect(document.querySelector('script')).toBeNull();
        fireEvent.click(screen.getAllByRole('button', { name: '查看聊天记录' }).at(-1)!);
        expect(await screen.findByText('内层内容')).toBeInTheDocument();
        expect(readForward).toHaveBeenNthCalledWith(2, { id: 'nested' });
    });
    it('retries failed audio resolution and creates a real audio control', async () => {
        const readRecord = vi
            .fn()
            .mockRejectedValueOnce(new Error('转码失败'))
            .mockResolvedValueOnce('data:audio/mpeg;base64,SUQzAA==');
        render(
            <NativeSegments
                api={{ readRecord }}
                segments={[{ type: 'record', data: { file: 'voice.silk' } }]}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '播放语音' }));
        expect(await screen.findByText('转码失败')).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '重试语音' }));
        expect(await screen.findByLabelText('语音播放器')).toHaveAttribute(
            'src',
            'data:audio/mpeg;base64,SUQzAA==',
        );
        expect(screen.getByLabelText('语音播放器')).toHaveAttribute('controls');
    });
    it('requests voice transcription after audio is resolved', async () => {
        const readRecord = vi.fn().mockResolvedValue('data:audio/mpeg;base64,SUQzAA==');
        const readRecordText = vi.fn().mockResolvedValue('你好，世界');
        render(
            <NativeSegments
                api={{ readRecord, readRecordText }}
                messageId="42"
                segments={[{ type: 'record', data: { file: 'voice.silk' } }]}
            />,
        );
        expect(screen.getByRole('button', { name: '转文字' })).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '转文字' }));
        expect(await screen.findByText('你好，世界')).toBeInTheDocument();
        expect(readRecordText).toHaveBeenCalledWith('42');
    });
    it('resolves an opaque video through the online media reader', async () => {
        const readVideo = vi.fn().mockResolvedValue('https://cdn.example/video.mp4');
        render(
            <NativeSegments
                api={{ readVideo }}
                segments={[{ type: 'video', data: { file: 'opaque-video' } }]}
            />,
        );
        expect(await screen.findByLabelText('视频播放器')).toHaveAttribute(
            'src',
            'https://cdn.example/video.mp4',
        );
        expect(readVideo).toHaveBeenCalledWith({ file: 'opaque-video' });
    });
    it('keeps a video player mounted when the context gets a new reader callback', async () => {
        const segments = [{ type: 'video', data: { file: 'stable-video' } }];
        const readVideo = vi.fn().mockResolvedValue('https://cdn.example/stable.mp4');
        const view = render(<NativeSegments api={{ readVideo }} segments={segments} />);
        const player = await screen.findByLabelText('视频播放器');
        const nextReader = vi.fn().mockResolvedValue('https://cdn.example/other.mp4');
        view.rerender(<NativeSegments api={{ readVideo: nextReader }} segments={segments} />);
        await waitFor(() => expect(screen.getByLabelText('视频播放器')).toBe(player));
        expect(player).toHaveAttribute('src', 'https://cdn.example/stable.mp4');
        expect(nextReader).not.toHaveBeenCalled();
    });
    it('allows an explicit retry after a direct video refresh failed', async () => {
        const data = { url: 'https://cdn.example/expired.mp4', file_id: 'video-id' };
        const readVideo = vi
            .fn()
            .mockRejectedValueOnce(new Error('临时网络错误'))
            .mockResolvedValueOnce('https://cdn.example/fresh.mp4');
        render(<NativeSegments api={{ readVideo }} segments={[{ type: 'video', data }]} />);
        fireEvent.error(screen.getByLabelText('视频播放器'));
        fireEvent.click(await screen.findByRole('button', { name: '重试视频' }));
        expect(await screen.findByLabelText('视频播放器')).toHaveAttribute(
            'src',
            'https://cdn.example/fresh.mp4',
        );
        expect(readVideo).toHaveBeenNthCalledWith(2, data, true);
    });
    it('does not replace a new image with an old pending media response', async () => {
        let finish!: (url: string) => void;
        const readImage = vi.fn(
            () =>
                new Promise<string>((resolve) => {
                    finish = resolve;
                }),
        );
        const view = render(
            <NativeSegments
                api={{ readImage }}
                segments={[{ type: 'image', data: { file: 'old-image' } }]}
            />,
        );
        await waitFor(() => expect(readImage).toHaveBeenCalledOnce());
        view.rerender(
            <NativeSegments
                api={{ readImage }}
                segments={[{ type: 'image', data: { url: 'https://cdn.example/new.png' } }]}
            />,
        );
        await act(async () => finish('https://cdn.example/old.png'));
        expect(screen.getByAltText('图片')).toHaveAttribute('src', 'https://cdn.example/new.png');
    });
    it('keeps a refreshed image URL after an unrelated context update', async () => {
        const segments = [
            {
                type: 'image',
                data: { file: 'cached-image', url: 'https://cdn.example/expired.png' },
            },
        ];
        const readImage = vi.fn().mockResolvedValue('https://cdn.example/fresh.png');
        const view = render(<NativeSegments api={{ readImage }} segments={segments} />);
        fireEvent.error(screen.getByAltText('图片'));
        await waitFor(() =>
            expect(screen.getByAltText('图片')).toHaveAttribute(
                'src',
                'https://cdn.example/fresh.png',
            ),
        );
        view.rerender(<NativeSegments api={{ readImage: vi.fn() }} segments={segments} />);
        expect(screen.getByAltText('图片')).toHaveAttribute('src', 'https://cdn.example/fresh.png');
    });
    it('keeps the same responsive dimensions while an image refresh is pending', async () => {
        let finish!: (url: string) => void;
        const readImage = vi.fn(
            () =>
                new Promise<string>((resolve) => {
                    finish = resolve;
                }),
        );
        render(
            <NativeSegments
                api={{ readImage }}
                segments={[
                    {
                        type: 'image',
                        data: {
                            file: 'responsive-image',
                            url: 'https://cdn.example/responsive.png',
                            width: 800,
                            height: 400,
                        },
                    },
                ]}
            />,
        );
        const style = screen.getByAltText('图片').parentElement!.getAttribute('style');
        fireEvent.error(screen.getByAltText('图片'));
        expect(screen.getByLabelText('正在读取图片').parentElement!.getAttribute('style')).toBe(
            style,
        );
        await act(async () => finish('https://cdn.example/responsive-fresh.png'));
        expect(screen.getByAltText('图片').parentElement!.getAttribute('style')).toBe(style);
    });
    it('bounds automatic refreshes and allows a new manual attempt', async () => {
        const data = { file: 'retryable-image', url: 'https://cdn.example/retryable.png' };
        const readImage = vi
            .fn()
            .mockResolvedValueOnce('https://cdn.example/retry1.png')
            .mockResolvedValueOnce('https://cdn.example/retry2.png')
            .mockResolvedValueOnce('https://cdn.example/retry3.png');
        render(<NativeSegments api={{ readImage }} segments={[{ type: 'image', data }]} />);
        fireEvent.error(screen.getByAltText('图片'));
        await waitFor(() =>
            expect(screen.getByAltText('图片')).toHaveAttribute(
                'src',
                'https://cdn.example/retry1.png',
            ),
        );
        fireEvent.error(screen.getByAltText('图片'));
        await waitFor(() =>
            expect(screen.getByAltText('图片')).toHaveAttribute(
                'src',
                'https://cdn.example/retry2.png',
            ),
        );
        fireEvent.error(screen.getByAltText('图片'));
        expect(await screen.findByRole('button', { name: '重试图片' })).toBeInTheDocument();
        expect(readImage).toHaveBeenCalledTimes(2);
        fireEvent.click(screen.getByRole('button', { name: '重试图片' }));
        await waitFor(() =>
            expect(screen.getByAltText('图片')).toHaveAttribute(
                'src',
                'https://cdn.example/retry3.png',
            ),
        );
    });
    it('reuses a successfully refreshed URL when virtualization remounts the image', async () => {
        const data = { file: 'remounted-image', url: 'https://cdn.example/remount-expired.png' };
        const readImage = vi.fn().mockResolvedValue('https://cdn.example/remount-fresh.png');
        const first = render(
            <NativeSegments
                api={{ readImage, mediaScope: 'account:1' }}
                segments={[{ type: 'image', data }]}
            />,
        );
        fireEvent.error(screen.getByAltText('图片'));
        await waitFor(() =>
            expect(screen.getByAltText('图片')).toHaveAttribute(
                'src',
                'https://cdn.example/remount-fresh.png',
            ),
        );
        Object.defineProperties(screen.getByAltText('图片'), {
            naturalWidth: { value: 800 },
            naturalHeight: { value: 400 },
        });
        fireEvent.load(screen.getByAltText('图片'));
        first.unmount();
        const second = render(
            <NativeSegments
                api={{ readImage, mediaScope: 'account:1' }}
                segments={[{ type: 'image', data }]}
            />,
        );
        expect(screen.getByAltText('图片')).toHaveAttribute(
            'src',
            'https://cdn.example/remount-fresh.png',
        );
        expect(readImage).toHaveBeenCalledOnce();
        second.unmount();
        render(
            <NativeSegments
                api={{ readImage, mediaScope: 'account:2' }}
                segments={[{ type: 'image', data }]}
            />,
        );
        expect(screen.getByAltText('图片')).toHaveAttribute(
            'src',
            'https://cdn.example/remount-expired.png',
        );
    });
    it('ignores a transcription returned after the displayed message changes', async () => {
        let finish!: (text: string) => void;
        const readRecordText = vi.fn(
            () =>
                new Promise<string>((resolve) => {
                    finish = resolve;
                }),
        );
        const readRecord = vi.fn();
        const view = render(
            <NativeSegments
                api={{ readRecord, readRecordText }}
                messageId="41"
                segments={[{ type: 'record', data: { file: 'first' } }]}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '转文字' }));
        view.rerender(
            <NativeSegments
                api={{ readRecord, readRecordText }}
                messageId="42"
                segments={[{ type: 'record', data: { file: 'second' } }]}
            />,
        );
        await act(async () => finish('上一条消息的文字'));
        expect(screen.queryByText('上一条消息的文字')).not.toBeInTheDocument();
        expect(screen.getByRole('button', { name: '转文字' })).toBeEnabled();
    });
});

describe('QQ face segments', () => {
    it('renders QQ faces inline with surrounding text', () => {
        render(
            <SegmentList
                mine={false}
                segments={[
                    { type: 'text', data: { text: '你好' } },
                    { type: 'face', data: { id: 14 } },
                    { type: 'text', data: { text: '！' } },
                ]}
            />,
        );
        const face = screen.getByRole('img', { name: 'QQ 表情 14' });
        expect(face).toHaveAttribute(
            'src',
            'https://koishi.js.org/QFace/assets/qq_emoji/14/png/14.png',
        );
        expect(face).toHaveAttribute('width', '24');
        expect(screen.getByText('你好')).toBeInTheDocument();
        expect(screen.getByText('！')).toBeInTheDocument();
    });

    it('keeps a readable fallback when an asset fails and retries a changed face', () => {
        const { rerender } = render(
            <SegmentList mine={false} segments={[{ type: 'face', data: { id: '277' } }]} />,
        );
        fireEvent.error(screen.getByRole('img', { name: 'QQ 表情 277' }));
        expect(screen.queryByRole('img')).not.toBeInTheDocument();
        expect(screen.getByText('表情 277')).toBeInTheDocument();
        rerender(<SegmentList mine={false} segments={[{ type: 'face', data: { id: '0' } }]} />);
        expect(screen.getByRole('img', { name: 'QQ 表情 0' })).toBeInTheDocument();
    });

    it('does not interpolate malformed identifiers into resource URLs', () => {
        render(
            <SegmentList
                mine={false}
                segments={[
                    { type: 'face', data: { id: '../external' } },
                    { type: 'face', data: {} },
                ]}
            />,
        );
        expect(screen.queryByRole('img')).not.toBeInTheDocument();
        expect(screen.getAllByText('表情')).toHaveLength(2);
    });
});
