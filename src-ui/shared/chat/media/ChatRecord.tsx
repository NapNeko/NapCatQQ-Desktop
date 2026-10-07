// 音频仅使用已转码的响应，失败保留原消息并允许重试。
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Check, LoaderCircle, Mic, Pause, Play, RotateCcw, Volume2, VolumeX } from 'lucide-react';
import { errorText } from '../../../core/domain/errors';
import { useChatView } from '../chatContext';
import './chat-media.css';
export function ChatRecord({
    data,
    fallback,
    messageId,
}: {
    data: Record<string, unknown>;
    fallback: ReactNode;
    messageId?: string;
}) {
    const { readRecord, readRecordText } = useChatView();
    const request = useRef(0);
    const textRequest = useRef(0);
    const audio = useRef<HTMLAudioElement>(null);
    const [source, setSource] = useState('');
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState('');
    const [playing, setPlaying] = useState(false);
    const [current, setCurrent] = useState(0);
    const [duration, setDuration] = useState(0);
    const [muted, setMuted] = useState(false);
    const initialTranscript = [data.transcript, data.text, data.summary, data.record_text].find(
        (value) => typeof value === 'string' && value.trim(),
    ) as string | undefined;
    const [transcript, setTranscript] = useState(initialTranscript);
    const [textLoading, setTextLoading] = useState(false);
    const [textError, setTextError] = useState('');
    useEffect(() => {
        request.current++;
        textRequest.current++;
        setSource('');
        setLoading(false);
        setError('');
        setPlaying(false);
        setCurrent(0);
        setDuration(0);
        setTranscript(initialTranscript);
        setTextLoading(false);
        setTextError('');
        return () => {
            request.current++;
            textRequest.current++;
        };
    }, [data, initialTranscript, messageId]);
    useEffect(() => {
        const node = audio.current;
        if (!node) return;
        node.onloadedmetadata = () =>
            setDuration(Number.isFinite(node.duration) ? node.duration : 0);
        node.ontimeupdate = () => setCurrent(node.currentTime);
        node.onplay = () => setPlaying(true);
        node.onpause = () => setPlaying(false);
        node.onended = () => {
            setPlaying(false);
            setCurrent(0);
        };
        node.onerror = () => {
            setSource('');
            setPlaying(false);
            setError('音频解码失败，请重试');
        };
        return () => {
            node.onloadedmetadata = null;
            node.ontimeupdate = null;
            node.onplay = null;
            node.onpause = null;
            node.onended = null;
            node.onerror = null;
        };
    }, [source]);
    const load = async () => {
        if (!readRecord || loading) return;
        const current = ++request.current;
        setLoading(true);
        setError('');
        setSource('');
        setPlaying(false);
        setCurrent(0);
        setDuration(0);
        try {
            const value = await readRecord(data);
            if (current === request.current) setSource(value);
        } catch (e) {
            if (current === request.current) setError(errorText(e));
        } finally {
            if (current === request.current) setLoading(false);
        }
    };
    const loadTranscript = async () => {
        if (!readRecordText || !messageId || textLoading || transcript) return;
        const current = ++textRequest.current;
        setTextLoading(true);
        setTextError('');
        try {
            const value = await readRecordText(messageId);
            if (current === textRequest.current) setTranscript(value);
        } catch (e) {
            if (current === textRequest.current) setTextError(errorText(e));
        } finally {
            if (current === textRequest.current) setTextLoading(false);
        }
    };
    const transcribe =
        readRecordText && messageId && !transcript ? (
            <button
                type="button"
                className="native-chat-voice-transcribe"
                disabled={textLoading}
                onClick={(event) => {
                    event.stopPropagation();
                    void loadTranscript();
                }}
            >
                {textLoading ? <LoaderCircle size={12} className="animate-spin" /> : '转文字'}
            </button>
        ) : null;
    if (!readRecord) return fallback;
    const toggle = async () => {
        const node = audio.current;
        if (!node) return;
        try {
            if (node.paused) await node.play();
            else node.pause();
        } catch {
            setError('播放被浏览器阻止，请再次点击');
        }
    };
    const seconds = (value: number) =>
        `${Math.floor(value / 60)}:${String(Math.floor(value % 60)).padStart(2, '0')}`;
    return (
        <span className="native-chat-voice">
            {source ? (
                <>
                    <audio
                        ref={audio}
                        aria-label="语音播放器"
                        controls
                        preload="metadata"
                        src={source}
                        className="native-chat-voice-audio"
                    />
                    <span className="native-chat-voice-player">
                        <button
                            type="button"
                            className="native-chat-voice-play"
                            aria-label={playing ? '暂停语音' : '播放语音'}
                            onClick={(event) => {
                                event.stopPropagation();
                                void toggle();
                            }}
                        >
                            {playing ? <Pause size={14} /> : <Play size={14} fill="currentColor" />}
                        </button>
                        <input
                            aria-label="语音进度"
                            type="range"
                            min={0}
                            max={duration || 1}
                            step={0.1}
                            value={Math.min(current, duration || 1)}
                            onChange={(event) => {
                                const value = Number(event.target.value);
                                if (audio.current) audio.current.currentTime = value;
                                setCurrent(value);
                            }}
                        />
                        <span className="native-chat-voice-time">
                            {seconds(current)} / {duration ? seconds(duration) : '--:--'}
                        </span>
                        <button
                            type="button"
                            className="native-chat-voice-mute"
                            aria-label={muted ? '取消静音' : '静音'}
                            onClick={(event) => {
                                event.stopPropagation();
                                setMuted((value) => {
                                    const next = !value;
                                    if (audio.current) audio.current.muted = next;
                                    return next;
                                });
                            }}
                        >
                            {muted ? <VolumeX size={13} /> : <Volume2 size={13} />}
                        </button>
                    </span>
                    {transcript ? (
                        <span className="native-chat-voice-transcript">
                            <Check size={12} />
                            {transcript}
                        </span>
                    ) : (
                        transcribe
                    )}
                </>
            ) : (
                <>
                    <button
                        type="button"
                        disabled={loading}
                        aria-label={loading ? '正在读取语音' : error ? '重试语音' : '播放语音'}
                        onClick={(event) => {
                            event.stopPropagation();
                            void load();
                        }}
                    >
                        {loading ? (
                            <LoaderCircle size={15} className="animate-spin" />
                        ) : error ? (
                            <RotateCcw size={14} />
                        ) : (
                            <Mic size={15} />
                        )}
                        {loading ? '正在读取语音…' : error ? '重试语音' : '播放语音'}
                    </button>
                    {transcribe}
                    {transcript && (
                        <span className="native-chat-voice-transcript">
                            <Check size={12} />
                            {transcript}
                        </span>
                    )}
                </>
            )}
            {error && (
                <span role="status" className="text-danger">
                    {error}
                </span>
            )}
            {textError && (
                <span role="status" className="text-danger">
                    {textError}
                </span>
            )}
        </span>
    );
}
