import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatViewContext, useChatView, type ChatViewApi } from '../chatContext';
import { ChatRecord } from './ChatRecord';

const source = 'data:audio/mpeg;base64,SUQzAA==';
const voice = { file: 'voice.silk', duration: 12 };
function Record({
    api,
    data = voice,
}: {
    api: Partial<ChatViewApi>;
    data?: Record<string, unknown>;
}) {
    return (
        <ChatViewContext.Provider value={{ ...useChatView(), ...api }}>
            <ChatRecord data={data} messageId="42" fallback={<span>语音</span>} />
        </ChatViewContext.Provider>
    );
}
function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((done) => {
        resolve = done;
    });
    return { promise, resolve };
}
function setDuration(node: HTMLAudioElement, duration: number) {
    Object.defineProperty(node, 'duration', { value: duration, configurable: true });
    fireEvent.loadedMetadata(node);
}

beforeEach(() => {
    vi.spyOn(HTMLMediaElement.prototype, 'play').mockImplementation(function (
        this: HTMLMediaElement,
    ) {
        Object.defineProperty(this, 'paused', { value: false, configurable: true });
        this.dispatchEvent(new Event('play'));
        this.dispatchEvent(new Event('playing'));
        return Promise.resolve();
    });
    vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(function (
        this: HTMLMediaElement,
    ) {
        Object.defineProperty(this, 'paused', { value: true, configurable: true });
        this.dispatchEvent(new Event('pause'));
    });
});

describe('voice playback controls', () => {
    it('shows controls before loading and starts playback on the first click with the selected settings', async () => {
        const pending = deferred<string>();
        const readRecord = vi.fn().mockReturnValue(pending.promise);
        render(<Record api={{ readRecord }} />);
        expect(screen.getByRole('slider', { name: '语音进度' })).toBeDisabled();
        expect(screen.getByText('0:00 / 0:12')).toBeInTheDocument();
        fireEvent.change(screen.getByRole('slider', { name: '语音音量' }), {
            target: { value: '0.4' },
        });
        fireEvent.click(screen.getByRole('button', { name: /播放速度/ }));
        fireEvent.click(screen.getByRole('button', { name: '播放语音' }));
        expect(screen.getByRole('button', { name: '正在读取语音' })).toBeDisabled();
        await act(async () => pending.resolve(source));
        const node = screen.getByLabelText('语音播放器') as HTMLAudioElement;
        expect(node.play).toHaveBeenCalledOnce();
        expect(node.volume).toBe(0.4);
        expect(node.playbackRate).toBe(1.25);
        expect(screen.getByRole('button', { name: '暂停语音' })).toBeInTheDocument();
        setDuration(node, 65.5);
        expect(screen.getByRole('slider', { name: '语音进度' })).toBeEnabled();
        expect(screen.getByText('0:00 / 1:05')).toBeInTheDocument();
        expect(readRecord).toHaveBeenCalledOnce();
    });

    it('pauses, resumes, seeks and adjusts rate, volume and mute on the real audio element', async () => {
        render(<Record api={{ readRecord: vi.fn().mockResolvedValue(source) }} />);
        fireEvent.click(screen.getByRole('button', { name: '播放语音' }));
        const node = (await screen.findByLabelText('语音播放器')) as HTMLAudioElement;
        setDuration(node, 60);
        fireEvent.click(screen.getByRole('button', { name: '暂停语音' }));
        expect(node.paused).toBe(true);
        fireEvent.change(screen.getByRole('slider', { name: '语音进度' }), {
            target: { value: '24' },
        });
        expect(node.currentTime).toBe(24);
        expect(screen.getByText('0:24 / 1:00')).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '播放语音' }));
        expect(node.play).toHaveBeenCalledTimes(2);
        fireEvent.click(screen.getByRole('button', { name: /播放速度/ }));
        expect(node.playbackRate).toBe(1.25);
        fireEvent.click(screen.getByRole('button', { name: /播放速度/ }));
        expect(node.playbackRate).toBe(1.5);
        fireEvent.click(screen.getByRole('button', { name: /播放速度/ }));
        expect(node.playbackRate).toBe(2);
        fireEvent.click(screen.getByRole('button', { name: /播放速度/ }));
        expect(node.playbackRate).toBe(1);
        fireEvent.click(screen.getByRole('button', { name: '静音' }));
        expect(node.muted).toBe(true);
        expect(screen.getByRole('slider', { name: '语音音量' })).toHaveValue('0');
        fireEvent.change(screen.getByRole('slider', { name: '语音音量' }), {
            target: { value: '0.25' },
        });
        expect(node.muted).toBe(false);
        expect(node.volume).toBe(0.25);
        fireEvent.change(screen.getByRole('slider', { name: '语音音量' }), {
            target: { value: '0' },
        });
        fireEvent.click(screen.getByRole('button', { name: '取消静音' }));
        expect(node.volume).toBe(1);
        fireEvent.waiting(node);
        expect(node.nextElementSibling).toHaveAttribute('aria-busy', 'true');
        fireEvent.playing(node);
        expect(node.nextElementSibling).toHaveAttribute('aria-busy', 'false');
        Object.defineProperty(node, 'ended', { value: true });
        Object.defineProperty(node, 'paused', { value: true, configurable: true });
        fireEvent.ended(node);
        fireEvent.click(screen.getByRole('button', { name: '播放语音' }));
        expect(node.currentTime).toBe(0);
        expect(node.play).toHaveBeenCalledTimes(3);
    });

    it('keeps a loaded voice and position when history recreates an equivalent segment', async () => {
        const readRecord = vi.fn().mockResolvedValue(source);
        const view = render(<Record api={{ readRecord, mediaScope: 'account-a' }} />);
        fireEvent.click(screen.getByRole('button', { name: '播放语音' }));
        const node = (await screen.findByLabelText('语音播放器')) as HTMLAudioElement;
        setDuration(node, 40);
        node.currentTime = 11;
        fireEvent.timeUpdate(node);
        view.rerender(<Record api={{ readRecord, mediaScope: 'account-a' }} data={{ ...voice }} />);
        expect(screen.getByLabelText('语音播放器')).toBe(node);
        expect(screen.getByText('0:11 / 0:40')).toBeInTheDocument();
        expect(node.pause).not.toHaveBeenCalled();
        expect(readRecord).toHaveBeenCalledOnce();
        view.rerender(<Record api={{ readRecord, mediaScope: 'account-b' }} data={{ ...voice }} />);
        expect(screen.queryByLabelText('语音播放器')).not.toBeInTheDocument();
        expect(node.pause).toHaveBeenCalledOnce();
    });

    it('ignores late audio and transcription responses after switching account', async () => {
        const oldAudio = deferred<string>();
        const oldText = deferred<string>();
        const readRecord = vi.fn().mockReturnValue(oldAudio.promise);
        const readRecordText = vi.fn().mockReturnValue(oldText.promise);
        const view = render(
            <Record api={{ readRecord, readRecordText, mediaScope: 'account-a' }} />,
        );
        fireEvent.click(screen.getByRole('button', { name: '播放语音' }));
        fireEvent.click(screen.getByRole('button', { name: '转文字' }));
        view.rerender(<Record api={{ readRecord, readRecordText, mediaScope: 'account-b' }} />);
        await act(async () => {
            oldAudio.resolve(source);
            oldText.resolve('旧账号转写');
        });
        expect(screen.queryByLabelText('语音播放器')).not.toBeInTheDocument();
        expect(screen.queryByText('旧账号转写')).not.toBeInTheDocument();
        expect(screen.getByRole('button', { name: '播放语音' })).toBeEnabled();
        expect(screen.getByRole('button', { name: '转文字' })).toBeEnabled();
        expect(HTMLMediaElement.prototype.play).not.toHaveBeenCalled();
    });

    it('retries resolution and decoding failures while retaining the controls', async () => {
        const readRecord = vi
            .fn()
            .mockRejectedValueOnce(new Error('转码失败'))
            .mockResolvedValue(source);
        render(<Record api={{ readRecord }} />);
        fireEvent.click(screen.getByRole('button', { name: '播放语音' }));
        expect(await screen.findByText('转码失败')).toBeInTheDocument();
        expect(screen.getByRole('slider', { name: '语音进度' })).toBeDisabled();
        fireEvent.click(screen.getByRole('button', { name: '重试语音' }));
        const node = await screen.findByLabelText('语音播放器');
        fireEvent.error(node);
        expect(screen.getByText('音频解码失败，请重试')).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '重试语音' }));
        expect(await screen.findByLabelText('语音播放器')).toHaveAttribute('src', source);
        expect(readRecord).toHaveBeenCalledTimes(3);
    });

    it('lets a user retry browser-blocked playback without reading the voice again', async () => {
        vi.mocked(HTMLMediaElement.prototype.play).mockRejectedValueOnce(
            new DOMException('blocked', 'NotAllowedError'),
        );
        const readRecord = vi.fn().mockResolvedValue(source);
        render(<Record api={{ readRecord }} />);
        fireEvent.click(screen.getByRole('button', { name: '播放语音' }));
        expect(await screen.findByText('播放被浏览器阻止，请再次点击播放')).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '播放语音' }));
        await waitFor(() =>
            expect(screen.getByRole('button', { name: '暂停语音' })).toBeInTheDocument(),
        );
        expect(screen.queryByRole('status')).not.toBeInTheDocument();
        expect(readRecord).toHaveBeenCalledOnce();
    });
});
