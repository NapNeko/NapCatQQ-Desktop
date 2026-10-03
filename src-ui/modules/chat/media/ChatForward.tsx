// 转发逐层按需读取，节点仍由现有安全消息段组件渲染。
import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { Clock3, Forward, Users } from 'lucide-react';
import { Dialog, DialogContent, DialogTitle } from '../../../shared/ui/Dialog';
import { errorText } from '../../../core/domain/errors';
import type { ForwardNode } from '../../../core/services/chat-media.service';
import type { Segment } from '../../../core/domain/debug/segments';
import { ChatAvatar } from '../ChatAvatar';
import './chat-media.css';
const ForwardDepth = createContext(0);
export function ChatForward({ data, read, renderSegments }: { data: Record<string, unknown>; read: (data: Record<string, unknown>) => Promise<ForwardNode[]>; renderSegments: (segments: Segment[]) => ReactNode }) {
    const depth = useContext(ForwardDepth); const reader = useRef(read); reader.current = read;
    const [open, setOpen] = useState(false); const [nodes, setNodes] = useState<ForwardNode[] | null>(null);
    const [error, setError] = useState(''); const [retry, setRetry] = useState(0);
    const previewCount = Array.isArray(data.messages) ? data.messages.length : Array.isArray(data.content) ? data.content.length : 0;
    const title = typeof data.title === 'string' && data.title.trim() ? data.title : '聊天记录';
    const summary = useMemo(() => {
        if (!nodes?.length) return null;
        const people = [...new Set(nodes.map(node => node.name).filter(Boolean))];
        const times = nodes.map(node => node.time).filter((time): time is number => typeof time === 'number');
        return { people, first: times.length ? Math.min(...times) : undefined, last: times.length ? Math.max(...times) : undefined };
    }, [nodes]);
    useEffect(() => {
        if (!open) return;
        let cancelled = false; setNodes(null); setError('');
        void reader.current(data).then(value => { if (!cancelled) setNodes(value); }).catch(e => { if (!cancelled) setError(errorText(e)); });
        return () => { cancelled = true; };
    }, [open, data, retry]);
    return <>
        <button type="button" aria-label="查看聊天记录" disabled={depth >= 5} className="native-chat-forward-card" onClick={event => { event.stopPropagation(); setOpen(true); }}>
            <span className="native-chat-forward-card-icon"><Forward size={18} /></span><span className="min-w-0"><span className="block truncate text-xs font-medium">{depth >= 5 ? '嵌套聊天记录层数过多' : title}</span><span className="mt-1 block text-2xs text-text-tertiary">{previewCount ? `${previewCount} 条消息` : '点击查看完整记录'}</span></span>
        </button>
        <Dialog open={open} onOpenChange={setOpen}><DialogContent className="native-chat-forward-dialog"><DialogTitle className="native-chat-forward-title"><span>{title}</span>{nodes && <span className="native-chat-forward-count">{nodes.length} 条</span>}</DialogTitle>
            <ForwardDepth.Provider value={depth + 1}><div className="native-chat-forward-body">
                {error ? (
                    <div role="status"><p className="text-sm text-danger">{error}</p><button className="native-chat-media-retry" onClick={() => setRetry(value => value + 1)}>重试读取聊天记录</button></div>
                ) : nodes === null ? (
                    <p role="status" className="py-6 text-sm text-text-tertiary">正在读取聊天记录…</p>
                ) : !nodes.length ? (
                    <p className="py-6 text-sm text-text-tertiary">这条聊天记录没有消息</p>
                ) : (
                    <>
                        <div className="native-chat-forward-summary">
                            <span><Users size={13} />{summary?.people.length ?? 0} 位参与者</span>
                            {summary?.first && <span><Clock3 size={13} />{new Date(summary.first).toLocaleDateString('zh-CN')}{summary.last && summary.last !== summary.first ? ` - ${new Date(summary.last).toLocaleDateString('zh-CN')}` : ''}</span>}
                        </div>
                        {nodes.map((node, index) => (
                            <article key={index} className="native-chat-forward-node">
                                <header><ChatAvatar contact={{ type: 'private', id: node.senderId, name: node.name }} small /><span className="font-medium text-text-secondary">{node.name}</span>{node.time && <time className="ml-auto">{new Date(node.time).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</time>}</header>
                                <div>{renderSegments(node.segments)}</div>
                            </article>
                        ))}
                    </>
                )}
            </div></ForwardDepth.Provider>
        </DialogContent></Dialog>
    </>;
}
