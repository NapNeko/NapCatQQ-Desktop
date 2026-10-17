// 音频仅使用已转码的响应，失败保留原消息并允许重试。
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { Check, LoaderCircle, Pause, Play, RotateCcw, Volume2, VolumeX } from 'lucide-react';
import { errorText } from '../../../core/domain/errors';
import { useChatView } from '../chatContext';
import './chat-media.css';

const playbackRates = [1, 1.25, 1.5, 2];
const seconds = (value: number) =>
    `${Math.floor(value / 60)}:${String(Math.floor(value % 60)).padStart(2, '0')}`;

export function ChatRecord({
    data,
    fallback,
    messageId,
}: {
    data: Record<string, unknown>;
    fallback: ReactNode;
    messageId?: string;
}) {
    const { mediaScope, readRecord } = useChatView();
    if (!readRecord) return fallback;
    // 历史合并会重建段对象；只有账号、消息或资源变化时才重置播放器。
    const identity = JSON.stringify([mediaScope, messageId, data.file, data.file_id, data.url]);
    return <RecordPlayer key={identity} data={data} messageId={messageId} />;
}

function RecordPlayer({ data, messageId }: { data: Record<string, unknown>; messageId?: string }) {
    const { readRecord, readRecordText } = useChatView();
    const request = useRef(0);
    const textRequest = useRef(0);
    const playAfterLoad = useRef(false);
    const audio = useRef<HTMLAudioElement>(null);
    const [source, setSource] = useState('');
    const [loading, setLoading] = useState(false);
    const [buffering, setBuffering] = useState(false);
    const [error, setError] = useState('');
    const [playing, setPlaying] = useState(false);
    const [current, setCurrent] = useState(0);
    const [duration, setDuration] = useState(0);
    const [volume, setVolume] = useState(1);
    const [muted, setMuted] = useState(false);
    const [rate, setRate] = useState(1);
    const initialTranscript = [data.transcript, data.text, data.summary, data.record_text].find(
        (value) => typeof value === 'string' && value.trim(),
    ) as string | undefined;
    const [transcript, setTranscript] = useState(initialTranscript);
    const [textLoading, setTextLoading] = useState(false);
    const [textError, setTextError] = useState('');
    useEffect(() => {
        if (initialTranscript) setTranscript(initialTranscript);
    }, [initialTranscript]);
    useEffect(
        () => () => {
            request.current++;
            textRequest.current++;
        },
        [],
    );
    useEffect(() => {
        const node = audio.current;
        if (!node) return;
        node.volume = volume;
        node.muted = muted;
        node.playbackRate = rate;
    }, [source, volume, muted, rate]);
    useEffect(() => {
        const node = audio.current;
        return () => node?.pause();
    }, [source]);
    const play = useCallback(async (node: HTMLAudioElement) => {
        const generation = request.current;
        try {
            await node.play();
        } catch (e) {
            if (generation !== request.current || audio.current !== node) return;
            setPlaying(false);
            setBuffering(false);
            // 暂停或更换资源会中止尚未完成的 play，不属于播放失败。
            if (e instanceof DOMException && e.name === 'AbortError') return;
            if (e instanceof DOMException && e.name === 'NotAllowedError') {
                setError('播放被浏览器阻止，请再次点击播放');
            } else {
                setSource('');
                setError('音频无法播放，请重试');
            }
        }
    }, []);
    useEffect(() => {
        if (!source || !audio.current || !playAfterLoad.current) return;
        playAfterLoad.current = false;
        void play(audio.current);
    }, [source, play]);

    const load = async () => {
        if (!readRecord || loading) return;
        const generation = ++request.current;
        setLoading(true);
        setError('');
        setCurrent(0);
        setDuration(0);
        try {
            const value = await readRecord(data);
            if (!value) throw new Error('协议未返回可播放音频，请重试');
            if (generation === request.current) {
                playAfterLoad.current = true;
                setSource(value);
            }
        } catch (e) {
            if (generation === request.current) setError(errorText(e));
        } finally {
            if (generation === request.current) setLoading(false);
        }
    };
    const toggle = () => {
        if (!source) {
            void load();
            return;
        }
        const node = audio.current;
        if (!node) return;
        setError('');
        if (node.paused) {
            if (node.ended) node.currentTime = 0;
            void play(node);
        } else node.pause();
    };
    const loadTranscript = async () => {
        if (!readRecordText || !messageId || textLoading || transcript) return;
        const generation = ++textRequest.current;
        setTextLoading(true);
        setTextError('');
        try {
            const value = await readRecordText(messageId);
            if (generation === textRequest.current) setTranscript(value);
        } catch (e) {
            if (generation === textRequest.current) setTextError(errorText(e));
        } finally {
            if (generation === textRequest.current) setTextLoading(false);
        }
    };
    const durationHint = Number(data.duration);
    const displayDuration =
        duration || (Number.isFinite(durationHint) && durationHint > 0 ? durationHint : 0);
    const silent = muted || volume === 0;
    const playLabel = loading
        ? '正在读取语音'
        : playing
          ? '暂停语音'
          : error && !source
            ? '重试语音'
            : '播放语音';
    return (
        <span className="native-chat-voice" onClick={(event) => event.stopPropagation()}>
            {source && (
                <audio
                    ref={audio}
                    aria-label="语音播放器"
                    preload="metadata"
                    src={source}
                    className="native-chat-voice-audio"
                    onLoadedMetadata={(event) => {
                        const value = event.currentTarget.duration;
                        setDuration(Number.isFinite(value) && value > 0 ? value : 0);
                    }}
                    onDurationChange={(event) => {
                        const value = event.currentTarget.duration;
                        setDuration(Number.isFinite(value) && value > 0 ? value : 0);
                    }}
                    onTimeUpdate={(event) => setCurrent(event.currentTarget.currentTime)}
                    onPlay={() => setPlaying(true)}
                    onPlaying={() => setBuffering(false)}
                    onWaiting={() => setBuffering(true)}
                    onCanPlay={() => setBuffering(false)}
                    onPause={() => {
                        setPlaying(false);
                        setBuffering(false);
                    }}
                    onEnded={() => {
                        setPlaying(false);
                        setBuffering(false);
                    }}
                    onError={() => {
                        setSource('');
                        setPlaying(false);
                        setBuffering(false);
                        setCurrent(0);
                        setDuration(0);
                        setError('音频解码失败，请重试');
                    }}
                />
            )}
            <span className="native-chat-voice-player" aria-busy={loading || buffering}>
                <button
                    type="button"
                    className="native-chat-voice-play"
                    aria-label={playLabel}
                    title={buffering ? '正在缓冲，点击暂停' : playLabel}
                    disabled={loading}
                    onClick={toggle}
                >
                    {loading || buffering ? (
                        <LoaderCircle size={15} className="animate-spin" />
                    ) : playing ? (
                        <Pause size={14} fill="currentColor" />
                    ) : error && !source ? (
                        <RotateCcw size={14} />
                    ) : (
                        <Play size={14} fill="currentColor" />
                    )}
                </button>
                <span className="native-chat-voice-track">
                    <input
                        className="native-chat-voice-progress"
                        aria-label="语音进度"
                        aria-valuetext={`${seconds(current)} / ${displayDuration ? seconds(displayDuration) : '未知时长'}`}
                        type="range"
                        min={0}
                        max={duration || 1}
                        step={0.1}
                        disabled={!source || !duration}
                        value={Math.min(current, duration || 1)}
                        onChange={(event) => {
                            const value = Number(event.target.value);
                            if (audio.current) audio.current.currentTime = value;
                            setCurrent(value);
                        }}
                    />
                    <span className="native-chat-voice-settings">
                        <span className="native-chat-voice-time">
                            {seconds(current)} /{' '}
                            {displayDuration ? seconds(displayDuration) : '--:--'}
                        </span>
                        <button
                            type="button"
                            className="native-chat-voice-rate"
                            aria-label={`播放速度：${rate} 倍，点击切换`}
                            title="切换播放速度"
                            onClick={() =>
                                setRate(
                                    playbackRates[
                                        (playbackRates.indexOf(rate) + 1) % playbackRates.length
                                    ],
                                )
                            }
                        >
                            {rate}×
                        </button>
                        <button
                            type="button"
                            className="native-chat-voice-mute"
                            aria-label={silent ? '取消静音' : '静音'}
                            aria-pressed={silent}
                            title={silent ? '取消静音' : '静音'}
                            onClick={() => {
                                if (silent) {
                                    setMuted(false);
                                    if (!volume) setVolume(1);
                                } else setMuted(true);
                            }}
                        >
                            {silent ? <VolumeX size={13} /> : <Volume2 size={13} />}
                        </button>
                        <input
                            className="native-chat-voice-volume"
                            aria-label="语音音量"
                            aria-valuetext={`${Math.round((muted ? 0 : volume) * 100)}%`}
                            type="range"
                            min={0}
                            max={1}
                            step={0.05}
                            value={muted ? 0 : volume}
                            onChange={(event) => {
                                setVolume(Number(event.target.value));
                                setMuted(false);
                            }}
                        />
                    </span>
                </span>
            </span>
            {transcript ? (
                <span className="native-chat-voice-transcript">
                    <Check size={12} />
                    {transcript}
                </span>
            ) : (
                readRecordText &&
                messageId && (
                    <button
                        type="button"
                        className="native-chat-voice-transcribe"
                        disabled={textLoading}
                        onClick={() => void loadTranscript()}
                    >
                        {textLoading && <LoaderCircle size={12} className="animate-spin" />}
                        {textLoading ? '正在转文字…' : '转文字'}
                    </button>
                )
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
