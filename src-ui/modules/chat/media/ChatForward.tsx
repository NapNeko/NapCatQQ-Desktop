// 嵌套记录在同一窗口导航，返回时复用内容和阅读位置。
import { createContext, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type Dispatch, type SetStateAction, type MutableRefObject } from 'react';
import { ArrowLeft, ChevronRight, Clock3, Users } from 'lucide-react';
import { Dialog, DialogContent, DialogTitle } from '../../../shared/ui/Dialog';
import { errorText } from '../../../core/domain/errors';
import type { ForwardNode } from '../../../core/services/chat-media.service';
import { messagePreview, type Segment } from '../../../core/domain/debug/segments';
import { ChatViewContext, useChatView } from '../../debug/right/chatContext';
import { CACHE_MAX, LruCache } from '../../debug/right/boundedCache';
import { ChatImageViewer } from '../ChatImageViewer';
import { ChatAvatar } from '../ChatAvatar';
import './chat-media.css';

type ForwardData = Record<string, unknown>;
interface Frame { data: ForwardData; title: string; nodes: ForwardNode[] | null; error: string; scroll: number }
interface Navigation { depth: number; contains: (data: ForwardData) => boolean; open: (data: ForwardData) => void }
const ForwardNavigation = createContext<Navigation | null>(null);
const resourceId = (data: ForwardData) => String(data.id ?? data.res_id ?? data.forward_id ?? data.message_id ?? '');
const titleOf = (data: ForwardData) => typeof data.title === 'string' && data.title.trim() ? data.title : '聊天记录';
const newFrame = (data: ForwardData): Frame => ({ data, title: titleOf(data), nodes: null, error: '', scroll: 0 });

// 卡片预览和弹窗共读一份：缓存按读取函数分区（每个账号一份），同一条记录只请求一次。
const forwardCache = new WeakMap<object, LruCache<ForwardNode[]>>();
const forwardInflight = new WeakMap<object, Map<string, Promise<ForwardNode[]>>>();
function readForwardCached(read: (data: ForwardData) => Promise<ForwardNode[]>, data: ForwardData): Promise<ForwardNode[]> {
    const key = resourceId(data);
    if (!key) return read(data);
    let cache = forwardCache.get(read);
    if (!cache) { cache = new LruCache(CACHE_MAX); forwardCache.set(read, cache); }
    const cached = cache.get(key);
    if (cached) return Promise.resolve(cached);
    let inflight = forwardInflight.get(read);
    if (!inflight) { inflight = new Map(); forwardInflight.set(read, inflight); }
    const pending = inflight.get(key);
    if (pending) return pending;
    const request = read(data).then(nodes => { cache.set(key, nodes); inflight.delete(key); return nodes; }, error => { inflight.delete(key); throw error; });
    inflight.set(key, request);
    return request;
}

export function ChatForward({ data, read, renderSegments }: { data: ForwardData; read: (data: ForwardData) => Promise<ForwardNode[]>; renderSegments: (segments: Segment[]) => ReactNode }) {
    const navigation = useContext(ForwardNavigation);
    const [open, setOpen] = useState(false);
    const [frames, setFrames] = useState<Frame[]>([]);
    const reader = useRef(read); reader.current = read;
    const limited = (navigation?.depth ?? 0) >= 5;
    const recursive = navigation?.contains(data) ?? false;
    const [nodes, setNodes] = useState<ForwardNode[] | null>(null);
    const resource = resourceId(data);
    const previewable = !limited && !recursive && !!resource;
    useEffect(() => {
        if (!previewable) return;
        let cancelled = false;
        void readForwardCached(reader.current, data).then(value => { if (!cancelled) setNodes(value); }).catch(() => {});
        return () => { cancelled = true; };
    }, [previewable, resource, data]);
    const previews = useMemo(() => (nodes ?? []).slice(0, 3).map(node => `${node.name}: ${messagePreview(node.segments) || '（空消息）'}`), [nodes]);
    return <>
        <button type="button" aria-label="查看聊天记录" disabled={limited || recursive} className="native-chat-forward-card" onClick={event => {
            event.stopPropagation();
            if (navigation) navigation.open(data);
            else { setFrames([newFrame(data)]); setOpen(true); }
        }}>
            <span className="native-chat-forward-card-title">{limited ? '嵌套聊天记录层数过多' : recursive ? '记录已在当前路径中' : titleOf(data)}</span>
            {previews.map((line, index) => <span key={index} className="native-chat-forward-line">{line}</span>)}
            <span className="native-chat-forward-card-foot">查看转发记录</span>
        </button>
        {!navigation && <ForwardDialog open={open} onOpenChange={setOpen} frames={frames} setFrames={setFrames} read={reader} renderSegments={renderSegments} />}
    </>;
}

function ForwardDialog({ open, onOpenChange, frames, setFrames, read, renderSegments }: {
    open: boolean; onOpenChange: (open: boolean) => void; frames: Frame[];
    setFrames: Dispatch<SetStateAction<Frame[]>>;
    read: MutableRefObject<(data: ForwardData) => Promise<ForwardNode[]>>;
    renderSegments: (segments: Segment[]) => ReactNode;
}) {
    const body = useRef<HTMLDivElement>(null);
    const frame = frames[frames.length - 1];
    const [image, showImage] = useState('');
    const view = useChatView();
    const mediaView = useMemo(() => ({ ...view, openImage: showImage }), [view]);
    const summary = useMemo(() => {
        if (!frame?.nodes?.length) return null;
        const people = new Set(frame.nodes.map(node => node.senderId || node.name));
        const times = frame.nodes.flatMap(node => typeof node.time === 'number' ? [node.time] : []);
        return { people: people.size, first: times.length ? Math.min(...times) : undefined, last: times.length ? Math.max(...times) : undefined };
    }, [frame?.nodes]);
    useEffect(() => {
        if (!open || !frame || frame.nodes !== null || frame.error) return;
        let cancelled = false;
        void readForwardCached(read.current, frame.data).then(nodes => {
            if (!cancelled) setFrames(path => path.map(item => item === frame ? { ...item, nodes } : item));
        }).catch(error => {
            if (!cancelled) setFrames(path => path.map(item => item === frame ? { ...item, error: errorText(error) } : item));
        });
        return () => { cancelled = true; };
    }, [open, frame, read, setFrames]);
    useLayoutEffect(() => { if (body.current) body.current.scrollTop = frame?.scroll ?? 0; }, [frames.length, frame?.nodes]);
    const remember = (path: Frame[]) => path.map((item, index) => index === path.length - 1 ? { ...item, scroll: body.current?.scrollTop ?? item.scroll } : item);
    const backTo = (index: number) => { showImage(''); setFrames(path => remember(path).slice(0, index + 1)); };
    const navigation: Navigation = {
        depth: frames.length,
        contains: data => frames.some(item => item.data === data || (resourceId(data) !== '' && resourceId(item.data) === resourceId(data))),
        open: data => { showImage(''); setFrames(path => [...remember(path), newFrame(data)]); },
    };
    return <Dialog open={open} onOpenChange={next => { if (!next) showImage(''); onOpenChange(next); }}>
        <DialogContent size="sheet" className="native-chat-forward-dialog" onEscapeKeyDown={event => {
            if (image) { event.preventDefault(); showImage(''); }
            else if (frames.length > 1) { event.preventDefault(); backTo(frames.length - 2); }
        }}>
            <DialogTitle className="native-chat-forward-title">
                {frames.length > 1 && <button type="button" className="native-chat-icon" aria-label="返回上一层聊天记录" onClick={() => backTo(frames.length - 2)}><ArrowLeft size={17} /></button>}
                <span>{frame?.title ?? '聊天记录'}</span>{frame?.nodes && <span className="native-chat-forward-count">{frame.nodes.length} 条</span>}
            </DialogTitle>
            {frames.length > 1 && <nav className="native-chat-forward-path" aria-label="聊天记录路径">{frames.map((item, index) => <span key={index} title={item.title}>{index > 0 && <ChevronRight size={12} />}<button type="button" aria-current={index === frames.length - 1 ? 'page' : undefined} onClick={() => backTo(index)}>{index === 0 ? '原始记录' : '第 ' + (index + 1) + ' 层'}</button></span>)}</nav>}
            <ForwardNavigation.Provider value={navigation}><ChatViewContext.Provider value={mediaView}>
                <div ref={body} className="native-chat-forward-body" aria-label="转发消息记录" aria-busy={frame?.nodes === null && !frame?.error}>
                    {frame?.error ? <div role="status"><p className="text-sm text-danger">{frame.error}</p><button type="button" className="native-chat-media-retry" onClick={() => setFrames(path => path.map(item => item === frame ? { ...item, error: '' } : item))}>重试读取聊天记录</button></div>
                        : !frame?.nodes ? <p role="status" className="py-6 text-sm text-text-tertiary">正在读取聊天记录…</p>
                        : !frame.nodes.length ? <p className="py-6 text-sm text-text-tertiary">这条聊天记录没有消息</p>
                        : <>
                            <div className="native-chat-forward-summary"><span><Users size={13} />{summary?.people ?? 0} 位参与者</span>{summary?.first && <span><Clock3 size={13} />{new Date(summary.first).toLocaleDateString('zh-CN')}{summary.last !== summary.first && ' - ' + new Date(summary.last!).toLocaleDateString('zh-CN')}</span>}</div>
                            {frame.nodes.map((node, index) => <article key={index} className="native-chat-forward-node">
                                <header><ChatAvatar contact={{ type: 'private', id: node.senderId, name: node.name }} small /><span className="font-medium text-text-secondary">{node.name}</span>{node.time && <time className="ml-auto">{new Date(node.time).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</time>}</header>
                                <div>{renderSegments(node.segments)}</div>
                            </article>)}
                        </>}
                </div>
                {image && <ChatImageViewer src={image} onClose={() => showImage('')} />}
            </ChatViewContext.Provider></ForwardNavigation.Provider>
        </DialogContent>
    </Dialog>;
}
