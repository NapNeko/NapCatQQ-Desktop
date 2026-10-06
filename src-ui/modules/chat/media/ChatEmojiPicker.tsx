// 收藏表情只在打开对应页时读取当前账号。
import {
    useEffect,
    useId,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
    type RefObject,
} from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { RefreshCw, Search, X } from 'lucide-react';
import { QQFace } from './QQFace';
import { QQ_FACE_FALLBACK, type QQSystemFace } from '../../../core/domain/chat/qqFaces';
import {
    QQ_CLASSIC_FACES,
    qqFaceService,
    type QQFaceCatalog,
} from '../../../core/services/qq-face.service';
import { chatMediaService } from '../../../core/services/chat-media.service';
import { errorText } from '../../../core/domain/errors';
import type { Attachment } from '../../../core/domain/chat/model';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import './chat-media.css';
export function ChatEmojiPicker({
    target,
    onSelect,
    disabledReason,
}: {
    target: DebugTarget;
    onSelect: (attachment: Attachment) => void;
    disabledReason: string;
}) {
    const [tab, setTab] = useState<'qq' | 'favorites'>('qq');
    const panelId = useId();
    const panel = useRef<HTMLDivElement>(null);
    const identity = `${target.backend}:${target.bot_id}:${target.qq_id}`;
    const [catalog, setCatalog] = useState<{ identity: string; value: QQFaceCatalog } | null>(null);
    const [faceLoading, setFaceLoading] = useState(false);
    const [faceRefresh, setFaceRefresh] = useState(0);
    const faces =
        catalog?.identity === identity
            ? catalog.value.faces
            : target.backend === 'snowluma'
              ? QQ_CLASSIC_FACES
              : QQ_FACE_FALLBACK;
    const [query, setQuery] = useState('');
    const [favorites, setFavorites] = useState<string[] | null>(null);
    const [error, setError] = useState('');
    const [retry, setRetry] = useState(0);
    useEffect(() => {
        if (disabledReason) return;
        let cancelled = false;
        setFaceLoading(true);
        void qqFaceService
            .forAccount(target, faceRefresh > 0)
            .then((value) => {
                if (!cancelled) setCatalog({ identity, value });
            })
            .finally(() => {
                if (!cancelled) setFaceLoading(false);
            });
        return () => {
            cancelled = true;
        };
    }, [identity, disabledReason, faceRefresh]);
    useLayoutEffect(() => {
        if (panel.current) panel.current.scrollTop = 0;
    }, [tab, query, target.bot_id, target.qq_id]);
    const filtered = useMemo(() => {
        const needle = query.trim().toLocaleLowerCase();
        return faces.filter((face) =>
            `${face.id} ${face.name} ${face.aliases?.join(' ') ?? ''}`
                .toLocaleLowerCase()
                .includes(needle),
        );
    }, [faces, query]);
    useEffect(() => {
        if (tab !== 'favorites' || disabledReason) return;
        let cancelled = false;
        setFavorites(null);
        setError('');
        void chatMediaService
            .favorites(target)
            .then((value) => {
                if (!cancelled) setFavorites(value);
            })
            .catch((e) => {
                if (!cancelled) setError(errorText(e));
            });
        return () => {
            cancelled = true;
        };
    }, [tab, target.bot_id, target.qq_id, disabledReason, retry]);
    return (
        <div className="native-chat-face-picker">
            <div
                role="tablist"
                aria-label="表情分类"
                className="native-chat-face-tabs"
                onKeyDown={(event) => {
                    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
                    event.preventDefault();
                    const next = tab === 'qq' ? 'favorites' : 'qq';
                    setTab(next);
                    event.currentTarget
                        .querySelector<HTMLButtonElement>('[data-tab=' + next + ']')
                        ?.focus();
                }}
            >
                {(['qq', 'favorites'] as const).map((value) => (
                    <button
                        key={value}
                        data-tab={value}
                        role="tab"
                        id={panelId + '-' + value}
                        aria-selected={tab === value}
                        aria-controls={panelId}
                        tabIndex={tab === value ? 0 : -1}
                        onClick={() => setTab(value)}
                    >
                        {value === 'qq' ? 'QQ 表情' : '收藏表情'}
                    </button>
                ))}
            </div>
            {tab === 'qq' && (
                <label className="native-chat-search native-chat-face-search">
                    <Search size={14} aria-hidden />
                    <input
                        aria-label="搜索 QQ 表情"
                        placeholder="搜索表情"
                        value={query}
                        onChange={(event) => setQuery(event.target.value)}
                    />
                    {query && (
                        <button
                            type="button"
                            aria-label="清除表情搜索"
                            onClick={() => setQuery('')}
                        >
                            <X size={13} />
                        </button>
                    )}
                </label>
            )}
            <div
                ref={panel}
                role="tabpanel"
                id={panelId}
                aria-labelledby={panelId + '-' + tab}
                className="native-chat-face-panel"
            >
                {tab === 'qq' ? (
                    filtered.length ? (
                        <QQFaceGrid
                            key={identity + query}
                            faces={filtered}
                            panel={panel}
                            onSelect={onSelect}
                            disabled={
                                !!disabledReason ||
                                (target.backend === 'snowluma' &&
                                    (faceLoading || catalog?.identity !== identity))
                            }
                        />
                    ) : (
                        <p role="status">没有找到表情</p>
                    )
                ) : disabledReason ? (
                    <p role="status">{disabledReason}</p>
                ) : error ? (
                    <div role="status">
                        <p>{error}</p>
                        <button
                            className="native-chat-media-retry"
                            onClick={() => setRetry((value) => value + 1)}
                        >
                            重试读取收藏表情
                        </button>
                    </div>
                ) : favorites === null ? (
                    <p role="status">正在读取收藏表情…</p>
                ) : !favorites.length ? (
                    <p role="status">当前账号没有收藏表情</p>
                ) : (
                    <FavoriteGrid favorites={favorites} panel={panel} onSelect={onSelect} />
                )}
            </div>
            {tab === 'qq' && (
                <div className="native-chat-face-footer">
                    <span role="status">
                        {faceLoading
                            ? '正在更新表情…'
                            : catalog?.identity === identity && catalog.value.limited
                              ? '暂时仅显示经典表情'
                              : `${faces.length} 个表情`}
                    </span>
                    <button
                        type="button"
                        aria-label="更新 QQ 表情"
                        title="重新读取当前账号的表情目录"
                        disabled={!!disabledReason || faceLoading}
                        onClick={() => setFaceRefresh((value) => value + 1)}
                    >
                        <RefreshCw size={12} />
                        更新
                    </button>
                </div>
            )}
        </div>
    );
}

function QQFaceGrid({
    faces,
    panel,
    onSelect,
    disabled,
}: {
    faces: QQSystemFace[];
    panel: RefObject<HTMLDivElement>;
    onSelect: (attachment: Attachment) => void;
    disabled: boolean;
}) {
    const virtual = useVirtualizer({
        count: Math.ceil(faces.length / 8),
        getScrollElement: () => panel.current,
        estimateSize: () => 40,
        overscan: 2,
        initialRect: { width: 340, height: 240 },
    });
    return (
        <div style={{ height: virtual.getTotalSize(), position: 'relative' }}>
            {virtual.getVirtualItems().map((row) => (
                <div
                    key={row.key}
                    className="native-chat-face-grid"
                    style={{ position: 'absolute', top: row.start, width: '100%' }}
                >
                    {faces.slice(row.index * 8, row.index * 8 + 8).map((face) => (
                        <button
                            key={face.id}
                            aria-label={'插入QQ 表情 ' + face.id}
                            title={face.name}
                            disabled={disabled}
                            onClick={() =>
                                onSelect({
                                    key: crypto.randomUUID(),
                                    type: 'face',
                                    id: face.id,
                                    name: face.name,
                                })
                            }
                        >
                            <QQFace id={face.id} name={face.name} url={face.url} size={28} />
                        </button>
                    ))}
                </div>
            ))}
        </div>
    );
}

function FavoriteGrid({
    favorites,
    panel,
    onSelect,
}: {
    favorites: string[];
    panel: RefObject<HTMLDivElement>;
    onSelect: (attachment: Attachment) => void;
}) {
    const virtual = useVirtualizer({
        count: Math.ceil(favorites.length / 4),
        getScrollElement: () => panel.current,
        estimateSize: () => 78,
        overscan: 2,
        initialRect: { width: 340, height: 240 },
    });
    return (
        <div style={{ height: virtual.getTotalSize(), position: 'relative' }}>
            {virtual.getVirtualItems().map((row) => (
                <div
                    key={row.key}
                    className="native-chat-favorite-grid"
                    style={{ position: 'absolute', top: row.start, width: '100%' }}
                >
                    {favorites.slice(row.index * 4, row.index * 4 + 4).map((url, index) => {
                        const name = '收藏表情 ' + (row.index * 4 + index + 1);
                        return (
                            <button
                                key={url}
                                aria-label={'插入' + name}
                                onClick={() =>
                                    onSelect({
                                        key: crypto.randomUUID(),
                                        type: 'image',
                                        path: url,
                                        subType: 1,
                                        name,
                                    })
                                }
                            >
                                <img
                                    src={url}
                                    alt={name}
                                    loading="lazy"
                                    decoding="async"
                                    referrerPolicy="no-referrer"
                                />
                            </button>
                        );
                    })}
                </div>
            ))}
        </div>
    );
}
