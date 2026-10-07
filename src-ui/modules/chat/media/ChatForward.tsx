// 嵌套记录在同一窗口导航，返回时复用内容和阅读位置。
import {
    createContext,
    useContext,
    useEffect,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
    type ReactNode,
    type Dispatch,
    type SetStateAction,
    type MutableRefObject,
} from 'react';
import { ArrowLeft, ChevronRight, Clock3, Users } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Dialog, DialogContent, DialogTitle } from '../../../shared/ui/Dialog';
import { errorText } from '../../../core/domain/errors';
import type { ForwardNode } from '../../../core/services/chat-media.service';
import { messagePreview, type Segment } from '../../../core/domain/debug/segments';
import { ChatViewContext, useChatView } from '../../debug/right/chatContext';
import { LruCache } from '../../debug/right/boundedCache';
import { ChatImageViewer } from '../ChatImageViewer';
import { ChatAvatar } from '../ChatAvatar';
import './chat-media.css';

type ForwardData = Record<string, unknown>;
interface Frame {
    data: ForwardData;
    title: string;
    nodes: ForwardNode[] | null;
    bytes: number;
    error: string;
    scroll: number;
}
interface Navigation {
    depth: number;
    contains: (data: ForwardData) => boolean;
    open: (data: ForwardData) => void;
}
const ForwardNavigation = createContext<Navigation | null>(null);
const resourceId = (data: ForwardData) =>
    String(data.id ?? data.res_id ?? data.forward_id ?? data.message_id ?? '');
const titleOf = (data: ForwardData) =>
    typeof data.title === 'string' && data.title.trim() ? data.title : '聊天记录';
const newFrame = (data: ForwardData): Frame => ({
    data,
    title: titleOf(data),
    nodes: null,
    bytes: 0,
    error: '',
    scroll: 0,
});

const FORWARD_BYTES = 8 * 1024 * 1024;
function forwardBytes(nodes: ForwardNode[]): number {
    // 序列化长度只用于预算，另留节点开销余量；不是 JS 堆大小。
    return JSON.stringify(nodes).length * 2 + nodes.length * 128;
}
const forwardCache = new LruCache<ForwardNode[]>(64, {
    maxBytes: FORWARD_BYTES,
    ttlMs: 5 * 60_000,
    sizeOf: forwardBytes,
});
const forwardInflight = new Map<string, Promise<ForwardNode[]>>();
let activeForwards = 0;
const forwardQueue: Array<() => void> = [];
async function readForwardLimited(
    read: (data: ForwardData) => Promise<ForwardNode[]>,
    data: ForwardData,
): Promise<ForwardNode[]> {
    if (activeForwards >= 4) {
        if (forwardQueue.length >= 64) throw new Error('聊天记录读取繁忙，请稍后重试');
        await new Promise<void>((resolve) => forwardQueue.push(resolve));
    } else activeForwards += 1;
    try {
        return await read(data);
    } finally {
        const next = forwardQueue.shift();
        if (next) next();
        else activeForwards -= 1;
    }
}
const readerIds = new WeakMap<object, number>();
let nextReaderId = 0;
function readerKey(read: object, scope?: string): string | number {
    if (scope) return scope;
    let key = readerIds.get(read);
    if (key === undefined) {
        key = ++nextReaderId;
        readerIds.set(read, key);
    }
    return key;
}
function trimFrames(path: Frame[]): Frame[] {
    let bytes = 0;
    return [...path]
        .reverse()
        .map((frame, index) => {
            bytes += frame.bytes;
            if (index > 0 && bytes > FORWARD_BYTES) {
                bytes -= frame.bytes;
                return { ...frame, nodes: null, bytes: 0 };
            }
            return frame;
        })
        .reverse();
}
function readForwardCached(
    read: (data: ForwardData) => Promise<ForwardNode[]>,
    data: ForwardData,
    scope?: string,
): Promise<ForwardNode[]> {
    const resource = resourceId(data);
    if (!resource) return readForwardLimited(read, data);
    const key = JSON.stringify([readerKey(read, scope), resource]);
    const cached = forwardCache.get(key);
    if (cached) return Promise.resolve(cached);
    const pending = forwardInflight.get(key);
    if (pending) return pending;
    if (forwardInflight.size >= 68)
        return Promise.reject(new Error('聊天记录读取繁忙，请稍后重试'));
    const request = readForwardLimited(read, data).then(
        (nodes) => {
            // 媒体字节和临时 blob 地址只跟随当前弹窗，不放进长期缓存。
            if (!/(?:data:|blob:|base64:\/\/)/i.test(JSON.stringify(nodes)))
                forwardCache.set(key, nodes);
            forwardInflight.delete(key);
            return nodes;
        },
        (error) => {
            forwardInflight.delete(key);
            throw error;
        },
    );
    forwardInflight.set(key, request);
    return request;
}

export function ChatForward({
    data,
    read,
    renderSegments,
}: {
    data: ForwardData;
    read: (data: ForwardData) => Promise<ForwardNode[]>;
    renderSegments: (segments: Segment[]) => ReactNode;
}) {
    const navigation = useContext(ForwardNavigation);
    const { mediaScope } = useChatView();
    const [open, setOpen] = useState(false);
    const [frames, setFrames] = useState<Frame[]>([]);
    const reader = useRef(read);
    reader.current = read;
    const limited = (navigation?.depth ?? 0) >= 5;
    const recursive = navigation?.contains(data) ?? false;
    const [previews, setPreviews] = useState<string[]>([]);
    const resource = resourceId(data);
    const previewable = !limited && !recursive && !!resource;
    useEffect(() => {
        setOpen(false);
        setFrames([]);
    }, [mediaScope, resource]);
    useEffect(() => {
        setPreviews([]);
        if (!previewable) return;
        let cancelled = false;
        void readForwardCached(reader.current, data, mediaScope)
            .then((value) => {
                if (!cancelled)
                    setPreviews(
                        value
                            .slice(0, 3)
                            .map((node) =>
                                `${node.name.slice(0, 64)}: ${messagePreview(node.segments) || '（空消息）'}`.slice(
                                    0,
                                    180,
                                ),
                            ),
                    );
            })
            .catch(() => {});
        return () => {
            cancelled = true;
        };
    }, [previewable, resource, data, mediaScope]);
    return (
        <>
            <button
                type="button"
                aria-label="查看聊天记录"
                disabled={limited || recursive}
                className="native-chat-forward-card"
                onClick={(event) => {
                    event.stopPropagation();
                    if (navigation) navigation.open(data);
                    else {
                        setFrames([newFrame(data)]);
                        setOpen(true);
                    }
                }}
            >
                <span className="native-chat-forward-card-title">
                    {limited
                        ? '嵌套聊天记录层数过多'
                        : recursive
                          ? '记录已在当前路径中'
                          : titleOf(data)}
                </span>
                {previews.map((line, index) => (
                    <span key={index} className="native-chat-forward-line">
                        {line}
                    </span>
                ))}
                <span className="native-chat-forward-card-foot">查看转发记录</span>
            </button>
            {!navigation && (
                <ForwardDialog
                    open={open}
                    onOpenChange={setOpen}
                    frames={frames}
                    setFrames={setFrames}
                    read={reader}
                    scope={mediaScope}
                    renderSegments={renderSegments}
                />
            )}
        </>
    );
}

interface ForwardMessagesProps {
    body: MutableRefObject<HTMLDivElement | null>;
    nodes: ForwardNode[];
    renderSegments: (segments: Segment[]) => ReactNode;
    header: ReactNode;
    initialOffset: number;
}
function ForwardMessages(props: ForwardMessagesProps) {
    if (props.nodes.length > 20) return <VirtualForwardMessages {...props} />;
    return (
        <>
            {props.header}
            {props.nodes.map((node, index) => (
                <ForwardMessage key={index} node={node} renderSegments={props.renderSegments} />
            ))}
        </>
    );
}
function VirtualForwardMessages({
    body,
    nodes,
    renderSegments,
    header,
    initialOffset,
}: ForwardMessagesProps) {
    const virtual = useVirtualizer({
        count: nodes.length + 1,
        getScrollElement: () => body.current,
        estimateSize: (index) => (index === 0 ? 48 : 110),
        overscan: 4,
        initialOffset,
        directDomUpdates: true,
    });
    return (
        <div ref={virtual.containerRef} style={{ position: 'relative' }}>
            {virtual.getVirtualItems().map((row) => (
                <div
                    key={row.key}
                    data-index={row.index}
                    ref={virtual.measureElement}
                    style={{ position: 'absolute', top: 0, left: 0, width: '100%' }}
                >
                    {row.index === 0 ? (
                        header
                    ) : (
                        <ForwardMessage
                            node={nodes[row.index - 1]}
                            renderSegments={renderSegments}
                        />
                    )}
                </div>
            ))}
        </div>
    );
}
function ForwardMessage({
    node,
    renderSegments,
}: {
    node: ForwardNode;
    renderSegments: (segments: Segment[]) => ReactNode;
}) {
    return (
        <article className="native-chat-forward-node">
            <header>
                <ChatAvatar
                    contact={{ type: 'private', id: node.senderId, name: node.name }}
                    small
                />
                <span className="font-medium text-text-secondary">{node.name}</span>
                {node.time && (
                    <time className="ml-auto">
                        {new Date(node.time).toLocaleTimeString('zh-CN', {
                            hour: '2-digit',
                            minute: '2-digit',
                        })}
                    </time>
                )}
            </header>
            <div>{renderSegments(node.segments)}</div>
        </article>
    );
}

function ForwardDialog({
    open,
    onOpenChange,
    frames,
    setFrames,
    read,
    scope,
    renderSegments,
}: {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    frames: Frame[];
    setFrames: Dispatch<SetStateAction<Frame[]>>;
    read: MutableRefObject<(data: ForwardData) => Promise<ForwardNode[]>>;
    scope?: string;
    renderSegments: (segments: Segment[]) => ReactNode;
}) {
    const body = useRef<HTMLDivElement>(null);
    const frame = frames[frames.length - 1];
    const [image, showImage] = useState('');
    const view = useChatView();
    const mediaView = useMemo(() => ({ ...view, openImage: showImage }), [view]);
    const summary = useMemo(() => {
        if (!frame?.nodes?.length) return null;
        const people = new Set(frame.nodes.map((node) => node.senderId || node.name));
        const times = frame.nodes.flatMap((node) =>
            typeof node.time === 'number' ? [node.time] : [],
        );
        return {
            people: people.size,
            first: times.length ? Math.min(...times) : undefined,
            last: times.length ? Math.max(...times) : undefined,
        };
    }, [frame?.nodes]);
    useEffect(() => {
        if (!open || !frame || frame.nodes !== null || frame.error) return;
        let cancelled = false;
        void readForwardCached(read.current, frame.data, scope)
            .then((nodes) => {
                if (!cancelled)
                    setFrames((path) =>
                        trimFrames(
                            path.map((item) =>
                                item === frame
                                    ? { ...item, nodes, bytes: forwardBytes(nodes) }
                                    : item,
                            ),
                        ),
                    );
            })
            .catch((error) => {
                if (!cancelled)
                    setFrames((path) =>
                        path.map((item) =>
                            item === frame ? { ...item, error: errorText(error) } : item,
                        ),
                    );
            });
        return () => {
            cancelled = true;
        };
    }, [open, frame, read, scope, setFrames]);
    useLayoutEffect(() => {
        if (body.current) body.current.scrollTop = frame?.scroll ?? 0;
    }, [frames.length, frame?.nodes]);
    const remember = (path: Frame[]) =>
        path.map((item, index) =>
            index === path.length - 1
                ? { ...item, scroll: body.current?.scrollTop ?? item.scroll }
                : item,
        );
    const backTo = (index: number) => {
        showImage('');
        setFrames((path) => remember(path).slice(0, index + 1));
    };
    const navigation: Navigation = {
        depth: frames.length,
        contains: (data) =>
            frames.some(
                (item) =>
                    item.data === data ||
                    (resourceId(data) !== '' && resourceId(item.data) === resourceId(data)),
            ),
        open: (data) => {
            showImage('');
            setFrames((path) => [...remember(path), newFrame(data)]);
        },
    };
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next) {
                    showImage('');
                    setFrames([]);
                }
                onOpenChange(next);
            }}
        >
            <DialogContent
                size="sheet"
                className="native-chat-forward-dialog"
                onEscapeKeyDown={(event) => {
                    if (image) {
                        event.preventDefault();
                        showImage('');
                    } else if (frames.length > 1) {
                        event.preventDefault();
                        backTo(frames.length - 2);
                    }
                }}
            >
                <DialogTitle className="native-chat-forward-title">
                    {frames.length > 1 && (
                        <button
                            type="button"
                            className="native-chat-icon"
                            aria-label="返回上一层聊天记录"
                            onClick={() => backTo(frames.length - 2)}
                        >
                            <ArrowLeft size={17} />
                        </button>
                    )}
                    <span>{frame?.title ?? '聊天记录'}</span>
                    {frame?.nodes && (
                        <span className="native-chat-forward-count">{frame.nodes.length} 条</span>
                    )}
                </DialogTitle>
                {frames.length > 1 && (
                    <nav className="native-chat-forward-path" aria-label="聊天记录路径">
                        {frames.map((item, index) => (
                            <span key={index} title={item.title}>
                                {index > 0 && <ChevronRight size={12} />}
                                <button
                                    type="button"
                                    aria-current={index === frames.length - 1 ? 'page' : undefined}
                                    onClick={() => backTo(index)}
                                >
                                    {index === 0 ? '原始记录' : '第 ' + (index + 1) + ' 层'}
                                </button>
                            </span>
                        ))}
                    </nav>
                )}
                <ForwardNavigation.Provider value={navigation}>
                    <ChatViewContext.Provider value={mediaView}>
                        <div
                            ref={body}
                            className="native-chat-forward-body"
                            aria-label="转发消息记录"
                            aria-busy={frame?.nodes === null && !frame?.error}
                        >
                            {frame?.error ? (
                                <div role="status">
                                    <p className="text-sm text-danger">{frame.error}</p>
                                    <button
                                        type="button"
                                        className="native-chat-media-retry"
                                        onClick={() =>
                                            setFrames((path) =>
                                                path.map((item) =>
                                                    item === frame ? { ...item, error: '' } : item,
                                                ),
                                            )
                                        }
                                    >
                                        重试读取聊天记录
                                    </button>
                                </div>
                            ) : !frame?.nodes ? (
                                <p role="status" className="py-6 text-sm text-text-tertiary">
                                    正在读取聊天记录…
                                </p>
                            ) : !frame.nodes.length ? (
                                <p className="py-6 text-sm text-text-tertiary">
                                    这条聊天记录没有消息
                                </p>
                            ) : (
                                <ForwardMessages
                                    key={frames.length}
                                    body={body}
                                    nodes={frame.nodes}
                                    renderSegments={renderSegments}
                                    initialOffset={frame.scroll}
                                    header={
                                        <div className="native-chat-forward-summary">
                                            <span>
                                                <Users size={13} />
                                                {summary?.people ?? 0} 位参与者
                                            </span>
                                            {summary?.first && (
                                                <span>
                                                    <Clock3 size={13} />
                                                    {new Date(summary.first).toLocaleDateString(
                                                        'zh-CN',
                                                    )}
                                                    {summary.last !== summary.first &&
                                                        ' - ' +
                                                            new Date(
                                                                summary.last!,
                                                            ).toLocaleDateString('zh-CN')}
                                                </span>
                                            )}
                                        </div>
                                    }
                                />
                            )}
                        </div>
                        {image && <ChatImageViewer src={image} onClose={() => showImage('')} />}
                    </ChatViewContext.Provider>
                </ForwardNavigation.Provider>
            </DialogContent>
        </Dialog>
    );
}
