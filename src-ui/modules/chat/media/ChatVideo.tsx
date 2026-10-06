import { useEffect, useRef, useState } from 'react';
import { Copy, Film, LoaderCircle, RotateCcw } from 'lucide-react';
import { useChatView } from '../../debug/right/chatContext';
import { useCopy } from '../../debug/right/rightParts';
import './chat-media.css';

const text = (value: unknown) =>
    typeof value === 'string' ? value : typeof value === 'number' ? String(value) : '';

function mediaUrl(data: Record<string, unknown>): string {
    for (const value of [data.url, data.file, data.path]) {
        const candidate = text(value);
        if (/^(https?:|data:video\/|blob:)/i.test(candidate)) return candidate;
    }
    return '';
}

export function ChatVideo({ data }: { data: Record<string, unknown> }) {
    const { openLink, readVideo } = useChatView();
    const reader = useRef(readVideo);
    reader.current = readVideo;
    const canRead = !!readVideo;
    const { copied, copy } = useCopy();
    const request = useRef(0);
    const direct = mediaUrl(data);
    const fileName = text(data.file_name) || text(data.name) || '视频';
    const [url, setUrl] = useState(direct);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState('');
    const [failed, setFailed] = useState(false);
    const [refreshAttempted, setRefreshAttempted] = useState(false);
    useEffect(() => {
        request.current++;
        setUrl(direct);
        setLoading(false);
        setError('');
        setFailed(false);
        setRefreshAttempted(false);
        if (direct || !reader.current) return;
        const current = request.current;
        setLoading(true);
        void reader
            .current(data)
            .then((value) => {
                if (current === request.current) setUrl(value);
            })
            .catch((e) => {
                if (current === request.current)
                    setError(e instanceof Error ? e.message : String(e));
            })
            .finally(() => {
                if (current === request.current) setLoading(false);
            });
        return () => {
            request.current++;
        };
    }, [data, direct, canRead]);
    const retry = () => {
        if (!readVideo || loading) return;
        const current = ++request.current;
        setLoading(true);
        setError('');
        setFailed(false);
        void readVideo(data, !!direct || refreshAttempted)
            .then((value) => {
                if (current === request.current) setUrl(value);
            })
            .catch((e) => {
                if (current === request.current)
                    setError(e instanceof Error ? e.message : String(e));
            })
            .finally(() => {
                if (current === request.current) setLoading(false);
            });
    };
    const refreshAfterPlaybackError = () => {
        if (!readVideo || refreshAttempted) {
            setFailed(true);
            return;
        }
        setRefreshAttempted(true);
        const current = ++request.current;
        setLoading(true);
        setError('');
        setFailed(false);
        void readVideo(data, true)
            .then((value) => {
                if (current === request.current) setUrl(value);
            })
            .catch((e) => {
                if (current === request.current) {
                    setUrl('');
                    setError(e instanceof Error ? e.message : String(e));
                }
            })
            .finally(() => {
                if (current === request.current) setLoading(false);
            });
    };
    if (!url || failed || loading || error) {
        const source = text(data.url) || text(data.file) || text(data.path);
        return (
            <span className="native-chat-video-fallback">
                {loading ? (
                    <LoaderCircle size={16} className="animate-spin" />
                ) : failed ? (
                    <Film size={16} />
                ) : error ? (
                    <RotateCcw size={16} />
                ) : (
                    <Film size={16} />
                )}
                <span>
                    {loading
                        ? '正在准备视频…'
                        : failed
                          ? '视频无法在线播放'
                          : error
                            ? error
                            : '视频需要可访问的在线播放地址'}
                </span>
                {error && readVideo && (
                    <button
                        type="button"
                        aria-label="重试视频"
                        title="重试"
                        onClick={(event) => {
                            event.stopPropagation();
                            retry();
                        }}
                    >
                        {<RotateCcw size={12} />}
                    </button>
                )}
                {source && (
                    <button
                        type="button"
                        aria-label="复制视频地址"
                        title="复制地址"
                        onClick={(event) => {
                            event.stopPropagation();
                            copy(source);
                        }}
                    >
                        {copied ? '已复制' : <Copy size={12} />}
                    </button>
                )}
            </span>
        );
    }
    return (
        <span className="native-chat-video">
            <video
                controls
                preload="metadata"
                playsInline
                src={url}
                onError={refreshAfterPlaybackError}
                aria-label="视频播放器"
            />
            <span className="native-chat-video-meta">
                <span title={fileName}>
                    <Film size={12} />
                    {fileName}
                </span>
                {/^https?:\/\//i.test(url) && (
                    <button
                        type="button"
                        onClick={(event) => {
                            event.stopPropagation();
                            void openLink(url);
                        }}
                        title="在浏览器中打开"
                    >
                        打开
                    </button>
                )}
            </span>
        </span>
    );
}
