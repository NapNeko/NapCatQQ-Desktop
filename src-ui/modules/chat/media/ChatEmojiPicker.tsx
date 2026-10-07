// 分类网格保留浏览位置，搜索仅定位与标记命中的表情。
import { useId, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ChevronDown, ChevronUp, LoaderCircle, RefreshCw, Search, X } from 'lucide-react';
import { QQFace } from '../../../shared/chat/media/QQFace';
import {
    qqFaceCategory,
    QQ_FACE_FALLBACK,
    type QQSystemFace,
} from '../../../core/domain/chat/qqFaces';
import {
    QQ_CLASSIC_FACES,
    useQQFaceCatalog,
    useRecentQQFaces,
} from '../../../hooks/chat/useChatQqFaces';
import { useFavoriteEmojis } from '../../../hooks/chat/useChatFavoriteStickers';
import type { FavoriteEmoji } from '../../../core/domain/chat/media';
import { useMotion, type MotionEnv } from '../../../hooks/preferences/useMotion';
import { errorText } from '../../../core/domain/errors';
import type { Attachment } from '../../../core/domain/chat/model';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import '../../../shared/chat/media/chat-media.css';

export function ChatEmojiPicker({
    target,
    onSelect,
    disabledReason,
}: {
    target: DebugTarget;
    onSelect: (attachment: Attachment) => void;
    disabledReason: string;
}) {
    const motion = useMotion();
    const [tab, setTab] = useState<'qq' | 'favorites'>('qq');
    const panelId = useId();
    const panel = useRef<HTMLDivElement>(null);
    const searchInput = useRef<HTMLInputElement>(null);
    const searchButton = useRef<HTMLButtonElement>(null);
    const identity = target.backend + ':' + target.bot_id + ':' + target.qq_id;
    const {
        catalog: readyCatalog,
        isLoading: catalogPending,
        isFetching: catalogFetching,
        refresh: refreshFaces,
    } = useQQFaceCatalog(target, !disabledReason);
    // pending 覆盖首载、fetching 覆盖手动刷新;禁用态下不显 busy,对齐原 effect 的 disabledReason 早退。
    const faceLoading = !disabledReason && (catalogPending || catalogFetching);
    const faces =
        readyCatalog?.faces ??
        (target.backend === 'snowluma' ? QQ_CLASSIC_FACES : QQ_FACE_FALLBACK);
    const [searchOpen, setSearchOpen] = useState(false);
    const [query, setQuery] = useState('');
    const [matchCursor, setMatchCursor] = useState(0);
    const [category, setCategory] = useState('全部');
    const { ids: recentIds, remember: rememberRecentFace } = useRecentQQFaces(identity);
    const categories = useMemo(
        () => [...new Set(['全部', '最近', ...faces.map(qqFaceCategory)])],
        [faces],
    );
    const selectedCategory = categories.includes(category) ? category : '全部';
    // 收藏查询开过一次 Tab 后保持挂载:订阅通知到达即后台刷新,回 Tab 直接见新数据,
    // 复现原"订阅 retry+1 → 回 Tab 读取"的观感且不会每次切 Tab 都发请求。
    const [favoritesOpened, setFavoritesOpened] = useState(false);
    const {
        favorites,
        isLoading: favoritePending,
        isFetching: favoriteFetching,
        error: favoriteFailure,
        refresh: refreshFavorites,
    } = useFavoriteEmojis(target, favoritesOpened && !disabledReason);
    const favoriteItems = favorites ?? null;
    const favoriteLoading =
        tab === 'favorites' && !disabledReason && (favoritePending || favoriteFetching);
    // 原实现在每次读取开始时清空错误;react-query 的 error 存续到下次成功,读取中隐藏以对齐。
    const error = favoriteFailure && !favoriteLoading ? errorText(favoriteFailure) : '';

    useLayoutEffect(() => {
        if (panel.current) panel.current.scrollTop = 0;
    }, [tab, selectedCategory, identity]);
    useLayoutEffect(() => {
        if (searchOpen) searchInput.current?.focus();
    }, [searchOpen]);

    const visibleFaces = useMemo(
        () =>
            selectedCategory === '最近'
                ? recentIds.flatMap((id) => {
                      const face = faces.find((value) => value.id === id);
                      return face ? [face] : [];
                  })
                : faces.filter(
                      (face) =>
                          selectedCategory === '全部' || qqFaceCategory(face) === selectedCategory,
                  ),
        [faces, selectedCategory, recentIds],
    );
    const hits = useMemo(() => {
        const needle = query.trim().toLocaleLowerCase();
        if (!needle) return [];
        if (tab === 'qq')
            return faces
                .filter((face) =>
                    (face.id + ' ' + face.name + ' ' + (face.aliases?.join(' ') ?? ''))
                        .toLocaleLowerCase()
                        .includes(needle),
                )
                .map((face) => ({
                    key: face.id,
                    label: face.name,
                    category: qqFaceCategory(face),
                }));
        return (favoriteItems ?? [])
            .filter((item) => item.description.toLocaleLowerCase().includes(needle))
            .map((item) => ({ key: item.url, label: item.description, category: '' }));
    }, [query, tab, faces, favoriteItems]);
    const hit = hits.length
        ? hits[((matchCursor % hits.length) + hits.length) % hits.length]
        : undefined;
    const matched = useMemo(() => new Set(hits.map((value) => value.key)), [hits]);
    useLayoutEffect(() => {
        if (tab === 'qq' && hit && !visibleFaces.some((face) => face.id === hit.key))
            setCategory(hit.category);
    }, [tab, hit?.key, visibleFaces]);
    const changeTab = (value: 'qq' | 'favorites') => {
        if (value === 'favorites') setFavoritesOpened(true);
        setTab(value);
        setQuery('');
        setMatchCursor(0);
    };
    const closeSearch = () => {
        setSearchOpen(false);
        setQuery('');
        setMatchCursor(0);
        searchButton.current?.focus();
    };
    const refreshing = tab === 'qq' ? faceLoading : favoriteLoading;
    return (
        <div className="native-chat-face-picker" data-motion={motion.enabled ? 'on' : 'off'}>
            <div className="native-chat-face-header">
                <div
                    role="tablist"
                    aria-label="表情分类"
                    className="native-chat-face-tabs"
                    onKeyDown={(event) => {
                        if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
                        event.preventDefault();
                        const next = tab === 'qq' ? 'favorites' : 'qq';
                        changeTab(next);
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
                            onClick={() => changeTab(value)}
                        >
                            {value === 'qq' ? 'QQ 表情' : '收藏表情'}
                        </button>
                    ))}
                </div>
                {searchOpen && (
                    <label className="native-chat-face-search">
                        <input
                            ref={searchInput}
                            aria-label={tab === 'qq' ? '搜索 QQ 表情' : '搜索收藏注释'}
                            placeholder={tab === 'qq' ? '名称 / 别名' : '搜索注释'}
                            value={query}
                            onChange={(event) => {
                                setQuery(event.target.value);
                                setMatchCursor(0);
                            }}
                            onKeyDown={(event) => {
                                if (event.key === 'Escape') {
                                    event.preventDefault();
                                    closeSearch();
                                } else if (event.key === 'Enter' && hits.length) {
                                    event.preventDefault();
                                    setMatchCursor((value) => value + (event.shiftKey ? -1 : 1));
                                }
                            }}
                        />
                    </label>
                )}
                <button
                    ref={searchButton}
                    type="button"
                    className="native-chat-face-search-toggle"
                    aria-label={searchOpen ? '关闭表情搜索' : '搜索表情'}
                    aria-expanded={searchOpen}
                    onClick={() => (searchOpen ? closeSearch() : setSearchOpen(true))}
                >
                    {searchOpen ? <X size={14} /> : <Search size={15} />}
                </button>
            </div>
            <div className="native-chat-face-browser-toolbar">
                {tab === 'qq' ? (
                    <div
                        role="group"
                        aria-label="QQ 表情分组"
                        className="native-chat-face-categories"
                    >
                        {categories.map((value) => (
                            <button
                                key={value}
                                type="button"
                                aria-pressed={selectedCategory === value}
                                onClick={() => setCategory(value)}
                            >
                                {value}
                            </button>
                        ))}
                    </div>
                ) : (
                    <span className="native-chat-face-count">
                        {favoriteItems ? favoriteItems.length + ' 个收藏' : '收藏表情'}
                    </span>
                )}
            </div>
            <div
                ref={panel}
                role="tabpanel"
                id={panelId}
                aria-labelledby={panelId + '-' + tab}
                className="native-chat-face-panel"
                aria-busy={tab === 'favorites' && favoriteLoading}
            >
                {tab === 'qq' ? (
                    visibleFaces.length ? (
                        <QQFaceGrid
                            key={identity + selectedCategory}
                            faces={visibleFaces}
                            panel={panel}
                            matched={matched}
                            currentKey={hit?.key}
                            motion={motion}
                            disabled={
                                !!disabledReason || (target.backend === 'snowluma' && !readyCatalog)
                            }
                            onSelect={(attachment) => {
                                if (attachment.type === 'face') rememberRecentFace(attachment.id);
                                onSelect(attachment);
                            }}
                        />
                    ) : (
                        <div className="native-chat-face-state" role="status">
                            {selectedCategory === '最近'
                                ? '还没有最近使用的表情'
                                : '当前分类没有表情'}
                        </div>
                    )
                ) : disabledReason ? (
                    <div className="native-chat-face-state" role="status">
                        {disabledReason}
                    </div>
                ) : error && favoriteItems === null ? (
                    <div className="native-chat-face-state" role="status">
                        <span>{error}</span>
                        <button className="native-chat-media-retry" onClick={refreshFavorites}>
                            重试读取收藏表情
                        </button>
                    </div>
                ) : favoriteItems === null ? (
                    <div
                        className="native-chat-face-loading"
                        role="status"
                        aria-label="正在读取收藏表情"
                    >
                        <div aria-hidden className="native-chat-face-skeleton-grid">
                            {Array.from({ length: 8 }, (_, index) => (
                                <span key={index} />
                            ))}
                        </div>
                        <span className="native-chat-face-loading-caption">
                            <LoaderCircle size={15} aria-hidden />
                            正在读取收藏表情…
                        </span>
                    </div>
                ) : !favoriteItems.length ? (
                    <div className="native-chat-face-state" role="status">
                        当前账号没有收藏表情
                    </div>
                ) : (
                    <FavoriteGrid
                        key={identity}
                        favorites={favoriteItems}
                        panel={panel}
                        matched={matched}
                        currentKey={hit?.key}
                        motion={motion}
                        onSelect={onSelect}
                    />
                )}
            </div>
            <div className="native-chat-face-footer">
                <span role="status" aria-live="polite" className="native-chat-face-result">
                    {query.trim()
                        ? hit
                            ? hit.label +
                              ' · ' +
                              ((((matchCursor % hits.length) + hits.length) % hits.length) + 1) +
                              '/' +
                              hits.length
                            : '没有找到表情'
                        : error && favoriteItems !== null
                          ? error
                          : tab === 'qq'
                            ? faceLoading
                                ? '正在更新表情…'
                                : readyCatalog?.limited
                                  ? '暂时仅显示经典表情'
                                  : faces.length + ' 个表情'
                            : favoriteLoading
                              ? '正在更新收藏…'
                              : favoriteItems
                                ? favoriteItems.length + ' 个收藏'
                                : ''}
                </span>
                {query.trim() ? (
                    <div className="native-chat-face-result-controls">
                        <button
                            type="button"
                            aria-label="上一个匹配表情"
                            disabled={!hits.length}
                            onClick={() => setMatchCursor((value) => value - 1)}
                        >
                            <ChevronUp size={13} />
                        </button>
                        <button
                            type="button"
                            aria-label="下一个匹配表情"
                            disabled={!hits.length}
                            onClick={() => setMatchCursor((value) => value + 1)}
                        >
                            <ChevronDown size={13} />
                        </button>
                    </div>
                ) : (
                    <button
                        type="button"
                        aria-label={tab === 'qq' ? '更新 QQ 表情' : '更新收藏表情'}
                        disabled={!!disabledReason || refreshing}
                        onClick={() => (tab === 'qq' ? refreshFaces() : refreshFavorites())}
                    >
                        <RefreshCw size={12} />
                        更新
                    </button>
                )}
            </div>
        </div>
    );
}

function useHitMotion(
    scope: RefObject<HTMLDivElement>,
    currentKey: string | undefined,
    visible: boolean,
    motion: MotionEnv,
) {
    useLayoutEffect(() => {
        if (!motion.enabled || !currentKey || !visible) return;
        const node = [
            ...(scope.current?.querySelectorAll<HTMLButtonElement>('[data-search-key]') ?? []),
        ].find((element) => element.dataset.searchKey === currentKey);
        if (!node) return;
        const tween = motion.fromTo(
            node,
            { opacity: 0.45 },
            { opacity: 1, duration: motion.duration('fast'), clearProps: 'opacity' },
        );
        return () => {
            tween?.kill();
            node.style.removeProperty('opacity');
        };
    }, [currentKey, visible, motion.enabled, motion.level, motion.speed]);
}

function QQFaceGrid({
    faces,
    panel,
    onSelect,
    disabled,
    matched,
    currentKey,
    motion,
}: {
    faces: QQSystemFace[];
    panel: RefObject<HTMLDivElement>;
    onSelect: (attachment: Attachment) => void;
    disabled: boolean;
    matched: ReadonlySet<string>;
    currentKey?: string;
    motion: MotionEnv;
}) {
    const scope = useRef<HTMLDivElement>(null);
    const virtual = useVirtualizer({
        count: Math.ceil(faces.length / 8),
        getScrollElement: () => panel.current,
        estimateSize: () => 40,
        overscan: 1,
        initialRect: { width: 340, height: 240 },
    });
    const currentIndex = currentKey ? faces.findIndex((face) => face.id === currentKey) : -1;
    useLayoutEffect(() => {
        if (currentIndex < 0) return;
        const frame = requestAnimationFrame(() => {
            virtual.scrollToIndex(Math.floor(currentIndex / 8), {
                align: 'center',
                behavior: motion.enabled ? 'smooth' : 'auto',
            });
        });
        return () => cancelAnimationFrame(frame);
    }, [currentKey, currentIndex, motion.enabled]);
    const rows = virtual.getVirtualItems();
    useHitMotion(
        scope,
        currentKey,
        rows.some((row) => row.index === Math.floor(currentIndex / 8)),
        motion,
    );
    return (
        <div ref={scope} style={{ height: virtual.getTotalSize(), position: 'relative' }}>
            {rows.map((row) => (
                <div
                    key={row.key}
                    className="native-chat-face-grid"
                    style={{ position: 'absolute', top: row.start, width: '100%' }}
                >
                    {faces.slice(row.index * 8, row.index * 8 + 8).map((face) => (
                        <button
                            key={face.id}
                            data-search-key={face.id}
                            data-search-match={matched.has(face.id)}
                            data-search-current={currentKey === face.id}
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
    matched,
    currentKey,
    motion,
}: {
    favorites: FavoriteEmoji[];
    panel: RefObject<HTMLDivElement>;
    onSelect: (attachment: Attachment) => void;
    matched: ReadonlySet<string>;
    currentKey?: string;
    motion: MotionEnv;
}) {
    const scope = useRef<HTMLDivElement>(null);
    const virtual = useVirtualizer({
        count: Math.ceil(favorites.length / 4),
        getScrollElement: () => panel.current,
        estimateSize: () => 78,
        overscan: 1,
        initialRect: { width: 340, height: 240 },
    });
    const currentIndex = currentKey ? favorites.findIndex((item) => item.url === currentKey) : -1;
    useLayoutEffect(() => {
        if (currentIndex < 0) return;
        const frame = requestAnimationFrame(() => {
            virtual.scrollToIndex(Math.floor(currentIndex / 4), {
                align: 'center',
                behavior: motion.enabled ? 'smooth' : 'auto',
            });
        });
        return () => cancelAnimationFrame(frame);
    }, [currentKey, currentIndex, motion.enabled]);
    const rows = virtual.getVirtualItems();
    useHitMotion(
        scope,
        currentKey,
        rows.some((row) => row.index === Math.floor(currentIndex / 4)),
        motion,
    );
    return (
        <div ref={scope} style={{ height: virtual.getTotalSize(), position: 'relative' }}>
            {rows.map((row) => (
                <div
                    key={row.key}
                    className="native-chat-favorite-grid"
                    style={{ position: 'absolute', top: row.start, width: '100%' }}
                >
                    {favorites.slice(row.index * 4, row.index * 4 + 4).map((item, index) => {
                        const name = '收藏表情 ' + (row.index * 4 + index + 1);
                        return (
                            <button
                                key={item.url}
                                data-search-key={item.url}
                                data-search-match={matched.has(item.url)}
                                data-search-current={currentKey === item.url}
                                aria-label={'插入' + name}
                                title={item.description || name}
                                onClick={() =>
                                    onSelect({
                                        key: crypto.randomUUID(),
                                        type: 'image',
                                        path: item.url,
                                        subType: 1,
                                        name: item.description || name,
                                    })
                                }
                            >
                                <img
                                    src={item.url}
                                    alt={item.description || name}
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
