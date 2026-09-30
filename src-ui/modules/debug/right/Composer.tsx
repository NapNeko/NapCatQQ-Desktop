// 轻量输入框：往当前会话发文字。输入 @ 弹出群成员（只有群聊），选中一条气泡就是回复它。
//
// 发往哪儿由右栏定：看着某个会话就是那个会话；在「全部」里就是选中的气泡所在的会话，没选中时置灰。
// 回车发送、Shift + 回车换行、输入法选字时的回车不算；Ctrl + 回车也发（并拦下，免得中栏的发送也跟着触发）。
// 发送途中还能接着打字：成功后只清掉发出去的那部分；失败时字都留着，下面写明原因。

import {
    memo,
    useCallback,
    useEffect,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
    type KeyboardEvent,
    type MouseEvent as ReactMouseEvent,
    type MutableRefObject,
} from 'react';
import { AtSign, CornerDownLeft, Reply, SendHorizontal, TriangleAlert, Users, X } from 'lucide-react';
import { cn } from '../../../shared/utils/cn';
import { Popover, PopoverAnchor, PopoverContent, Spinner, Tooltip, TooltipContent, TooltipTrigger } from '../../../shared/ui';
import { IconAction } from './rightParts';
import { useDebugCall } from '../../../hooks/debug/useDebugCall';
import { useDebugContacts, type DebugContactOption } from '../../../hooks/debug/useDebugContacts';
import { debugErrorCopy, retcodeHint } from '../../../core/domain/debug/errorCopy';
import type { SessionKey } from '../../../core/domain/debug/chat';
import type { DebugCallResponse } from '../../../core/ipc/generated/debug/DebugCallResponse';
import type { DebugChannelId } from '../../../core/ipc/generated/debug/DebugChannelId';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import {
    EMPTY_DRAFT,
    buildMessageSegments,
    hasContent,
    mentionLabel,
    mentionQueryAt,
    pruneMentions,
    remainderAfterSend,
    type ComposerDraft,
    type Mention,
} from '../../../core/domain/debug/composerModel';
import { draftKey, readDraft, writeDraft } from './composerDrafts';

export interface ComposerTarget {
    session: SessionKey;
    type: 'group' | 'private';
    id: number;
    name: string;
}

export interface ComposerReply {
    messageId: number;
    senderName: string;
    preview: string;
}

export interface ComposerProps {
    target: DebugTarget | null;
    callChannel: DebugChannelId;
    /** 发往哪个会话；null 时置灰 */
    to: ComposerTarget | null;
    /** 在「全部」视图：在输入框上面写明发到哪 */
    showTarget: boolean;
    reply: ComposerReply | null;
    onClearReply: () => void;
    /** 「全部」视图里点掉「发到 X」：不再往那个会话发 */
    onDismissTarget: () => void;
    /** 发出去了：右栏据此取消选中、回到底部 */
    onSent: () => void;
    /** 右栏点了「回复」后把光标放进输入框 */
    focusRef?: MutableRefObject<(() => void) | null>;
}

const SEND_TIMEOUT_MS = 30_000;
const LINE_PX = 20;
const MAX_LINES = 6;
const PAD_PX = 8;
const MAX_OPTIONS = 50;

interface PickerState {
    start: number;
    query: string;
    /** 用户用 ↑↓ / 鼠标挑中的那一项；-1 是还没挑，默认落在第一个成员上（不会默认落在「全体成员」上） */
    index: number;
}

interface PickOption {
    qq: string;
    label: string;
    hint?: string;
}

function outcomeError(res: DebugCallResponse): string | null {
    if (res.result.kind === 'err') {
        const copy = debugErrorCopy(res.result.error);
        return copy.detail ? `${copy.title}：${copy.detail}` : copy.title;
    }
    const o = res.result.outcome;
    if (o.ok) return null;
    const why = o.wording || o.message || retcodeHint(o.retcode) || '';
    return `没发出去：retcode ${o.retcode}${why ? ` · ${why}` : ''}`;
}

export const Composer = memo(function Composer({
    target,
    callChannel,
    to,
    showTarget,
    reply,
    onClearReply,
    onDismissTarget,
    onSent,
    focusRef,
}: ComposerProps) {
    const botId = target?.bot_id ?? null;
    const running = target?.running ?? false;
    const key = botId && to ? draftKey(botId, to.session) : null;
    const { send } = useDebugCall();

    const [draft, setDraftState] = useState<ComposerDraft>(() => (key ? readDraft(key) : EMPTY_DRAFT));
    const draftRef = useRef(draft);
    draftRef.current = draft;
    const keyRef = useRef(key);
    const [loadedKey, setLoadedKey] = useState(key);
    // 换了会话：换成那个会话的草稿（渲染时对齐，不多闪一帧旧草稿）
    if (loadedKey !== key) {
        setLoadedKey(key);
        setDraftState(key ? readDraft(key) : EMPTY_DRAFT);
    }
    keyRef.current = key;

    const [sending, setSending] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [picker, setPicker] = useState<PickerState | null>(null);
    const areaRef = useRef<HTMLTextAreaElement>(null);

    useEffect(() => {
        setError(null);
        setPicker(null);
    }, [key]);

    useEffect(() => {
        if (!focusRef) return;
        focusRef.current = () => areaRef.current?.focus();
        return () => {
            focusRef.current = null;
        };
    }, [focusRef]);

    const setDraft = useCallback((next: ComposerDraft) => {
        setDraftState(next);
        if (keyRef.current) writeDraft(keyRef.current, next);
    }, []);

    // ---- 高度跟着内容长，最多 6 行
    useLayoutEffect(() => {
        const el = areaRef.current;
        if (!el) return;
        el.style.height = 'auto';
        el.style.height = `${Math.min(el.scrollHeight, MAX_LINES * LINE_PX + PAD_PX)}px`;
    }, [draft.text]);

    const isGroup = to?.type === 'group';
    const canCompose = !!to && running;

    // ---- @ 成员：只在弹出时才去拉（拉过的缓存 5 分钟）
    const contacts = useDebugContacts(target, 'member', picker && isGroup ? to.id : null);
    const options = useMemo<PickOption[]>(() => {
        if (!picker) return [];
        const q = picker.query.trim().toLowerCase();
        const out: PickOption[] = [];
        if (q === '' || '全体成员'.includes(q) || 'all'.startsWith(q)) out.push({ qq: 'all', label: '全体成员', hint: '需要管理员权限' });
        const matched = contacts.options.filter(
            (o: DebugContactOption) =>
                q === '' || o.label.toLowerCase().includes(q) || String(o.id).startsWith(q) || (o.hint ?? '').toLowerCase().includes(q),
        );
        for (const o of matched.slice(0, MAX_OPTIONS)) out.push({ qq: String(o.id), label: o.label, hint: o.hint });
        // 拉不到成员、或者名单里没有：直接 @ 一个号
        if (/^\d{5,12}$/.test(q) && !out.some((o) => o.qq === q)) out.push({ qq: q, label: q, hint: '直接 @ 这个 QQ 号' });
        return out;
    }, [picker, contacts.options]);
    const moreCount = picker ? Math.max(0, contacts.options.length - MAX_OPTIONS) : 0;
    // 默认高亮第一个真人：回车 / Tab 不能一不小心 @ 了全体
    const activeIndex = !picker
        ? -1
        : picker.index >= 0 && picker.index < options.length
          ? picker.index
          : options.findIndex((o) => o.qq !== 'all');

    const refreshPicker = useCallback(
        (text: string, caret: number | null) => {
            if (!isGroup || caret === null) {
                setPicker(null);
                return;
            }
            const m = mentionQueryAt(text, caret);
            setPicker((prev) => (m ? { start: m.start, query: m.query, index: prev && prev.start === m.start ? prev.index : -1 } : null));
        },
        [isGroup],
    );

    const insertMention = (opt: PickOption) => {
        const el = areaRef.current;
        if (!picker || !el) return;
        const caret = el.selectionStart ?? draft.text.length;
        // 群里重名的人加上 QQ 号区分，每个「@名字」都认得出是谁
        const sameName = contacts.options.filter((o) => o.label === opt.label).length;
        const label = mentionLabel(opt.label, opt.qq, draft.mentions, sameName);
        const text = `${draft.text.slice(0, picker.start)}${label} ${draft.text.slice(caret)}`;
        const mentions: Mention[] = [...draft.mentions.filter((m) => m.label !== label || m.qq === opt.qq), { qq: opt.qq, label }];
        setDraft({ text, mentions: dedupe(mentions) });
        setPicker(null);
        const pos = picker.start + label.length + 1;
        requestAnimationFrame(() => {
            el.focus();
            el.setSelectionRange(pos, pos);
        });
    };

    // ---- 发送
    const segments = useMemo(
        () => buildMessageSegments(draft.text, draft.mentions, reply?.messageId),
        [draft.text, draft.mentions, reply?.messageId],
    );
    const sendable = canCompose && !sending && hasContent(segments);

    const submit = async () => {
        if (!sendable || !botId || !to) return;
        const sentText = draft.text;
        const sentKey = key;
        setSending(true);
        setError(null);
        const res = await send(null, {
            bot_id: botId,
            channel: callChannel,
            action: to.type === 'group' ? 'send_group_msg' : 'send_private_msg',
            params: to.type === 'group' ? { group_id: to.id, message: segments } : { user_id: to.id, message: segments },
            origin: 'composer',
            timeout_ms: SEND_TIMEOUT_MS,
        });
        setSending(false);
        const problem = outcomeError(res);
        if (problem) {
            if (keyRef.current === sentKey) setError(problem);
            return;
        }
        // 发送途中换了会话：只收拾原来那个会话的草稿
        if (keyRef.current !== sentKey) {
            if (sentKey) {
                const old = readDraft(sentKey);
                const text = remainderAfterSend(old.text, sentText);
                writeDraft(sentKey, { text, mentions: pruneMentions(text, old.mentions) });
            }
            return;
        }
        const current = draftRef.current;
        const text = remainderAfterSend(current.text, sentText);
        setDraft({ text, mentions: pruneMentions(text, current.mentions) });
        onSent();
    };

    const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
        // 输入法选字中的回车：有的 webview 不给 isComposing，只给 keyCode 229
        if (e.nativeEvent.isComposing || e.keyCode === 229) return;
        if (picker) {
            if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && options.length > 0) {
                e.preventDefault();
                const d = e.key === 'ArrowDown' ? 1 : -1;
                const from = activeIndex < 0 ? (d > 0 ? -1 : 0) : activeIndex;
                setPicker({ ...picker, index: (from + d + options.length) % options.length });
                return;
            }
            if (e.key === 'Enter' || e.key === 'Tab') {
                // 弹层开着时回车只管选人，不发送；没有可选的人（名单还没到）就先收起弹层
                e.preventDefault();
                const opt = activeIndex >= 0 ? options[activeIndex] : undefined;
                if (opt) insertMention(opt);
                else setPicker(null);
                return;
            }
        }
        if (e.key === 'Escape') {
            if (picker) {
                e.preventDefault();
                setPicker(null);
            } else if (reply) {
                e.preventDefault();
                onClearReply();
            }
            return;
        }
        if (e.key === 'Enter' && (!e.shiftKey || e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            void submit();
            return;
        }
        // 退格紧挨着一个 @：整个删掉，和 QQ 一样
        if (e.key === 'Backspace') {
            const el = e.currentTarget;
            if (el.selectionStart !== el.selectionEnd) return;
            const before = draft.text.slice(0, el.selectionStart);
            const hit = draft.mentions.find((m) => before.endsWith(`${m.label} `) || before.endsWith(m.label));
            if (!hit) return;
            const cut = before.endsWith(`${hit.label} `) ? hit.label.length + 1 : hit.label.length;
            e.preventDefault();
            const pos = el.selectionStart - cut;
            const text = draft.text.slice(0, pos) + draft.text.slice(el.selectionStart);
            setDraft({ text, mentions: pruneMentions(text, draft.mentions) });
            requestAnimationFrame(() => el.setSelectionRange(pos, pos));
        }
    };

    const placeholder = !target
        ? '先选一个 Bot'
        : !running
          ? 'Bot 没在运行，启动后才能发消息'
          : !to
            ? '先选一个会话，或点一条消息'
            : `发到「${to.name}」，Enter 发送`;

    const pickerOpen = !!picker && canCompose;

    return (
        <div className="shrink-0 border-t border-border-subtle/70 px-2 pb-2 pt-1.5">
            {(reply || (showTarget && to)) && (
                <div className="mb-1 flex min-w-0 items-center gap-1.5 px-0.5">
                    {showTarget && to && (
                        <span className="inline-flex min-w-0 max-w-[45%] shrink-0 items-center gap-1 rounded-pill bg-inset py-0.5 pl-2 pr-0.5 text-2xs text-text-secondary">
                            {to.type === 'group' ? (
                                <Users size={10} aria-hidden className="shrink-0" />
                            ) : (
                                <CornerDownLeft size={10} aria-hidden className="shrink-0" />
                            )}
                            <span className="truncate">发到 {to.name}</span>
                            <ChipClose label="不发到这里" onClick={onDismissTarget} />
                        </span>
                    )}
                    {reply && (
                        <span className="inline-flex min-w-0 flex-1 items-center gap-1 rounded-pill bg-brand-soft px-2 py-0.5 text-2xs text-brand">
                            <Reply size={10} aria-hidden className="shrink-0" />
                            <span className="min-w-0 flex-1 truncate">
                                回复 {reply.senderName}：{reply.preview || '（空消息）'}
                            </span>
                            <ChipClose label="不回复了" onClick={onClearReply} className="hover:bg-brand/15" />
                        </span>
                    )}
                </div>
            )}
            <Popover open={pickerOpen} onOpenChange={(open) => !open && setPicker(null)}>
                <PopoverAnchor asChild>
                    <div
                        className={cn(
                            'flex items-end gap-1 rounded-md border bg-surface px-1.5 py-1 transition-colors',
                            canCompose ? 'border-border-subtle focus-within:border-brand/45' : 'border-border-subtle/60 bg-inset/40',
                            error && 'border-danger/40',
                        )}
                    >
                        {isGroup && (
                            <IconAction
                                label="@ 群成员"
                                tip="@ 群成员（也可以直接输入 @）"
                                disabled={!canCompose}
                                onMouseDown={(e) => e.preventDefault()}
                                onClick={() => {
                                    const el = areaRef.current;
                                    if (!el) return;
                                    const caret = el.selectionStart ?? draft.text.length;
                                    const needsSpace = caret > 0 && !/\s$/.test(draft.text.slice(0, caret));
                                    const insert = `${needsSpace ? ' ' : ''}@`;
                                    const text = draft.text.slice(0, caret) + insert + draft.text.slice(caret);
                                    setDraft({ ...draft, text });
                                    const pos = caret + insert.length;
                                    setPicker({ start: pos - 1, query: '', index: -1 });
                                    requestAnimationFrame(() => {
                                        el.focus();
                                        el.setSelectionRange(pos, pos);
                                    });
                                }}
                                className="mb-px"
                            >
                                <AtSign size={14} aria-hidden />
                            </IconAction>
                        )}
                        <textarea
                            ref={areaRef}
                            rows={1}
                            value={draft.text}
                            disabled={!canCompose}
                            placeholder={placeholder}
                            title={canCompose ? `Enter 发送，Shift + Enter 换行${isGroup ? '，输入 @ 提到群成员' : ''}` : undefined}
                            aria-label={to ? `发到 ${to.name} 的消息` : '消息'}
                            onChange={(e) => {
                                const text = e.target.value;
                                setDraft({ text, mentions: pruneMentions(text, draft.mentions) });
                                refreshPicker(text, e.target.selectionStart);
                            }}
                            onKeyDown={onKeyDown}
                            onKeyUp={(e) => {
                                if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) {
                                    refreshPicker(e.currentTarget.value, e.currentTarget.selectionStart);
                                }
                            }}
                            onClick={(e) => refreshPicker(e.currentTarget.value, e.currentTarget.selectionStart)}
                            className="min-h-[28px] min-w-0 flex-1 resize-none bg-transparent px-1 py-1 text-[13px] leading-5 text-text outline-none placeholder:text-text-disabled disabled:cursor-not-allowed"
                        />
                        <Tooltip>
                            <TooltipTrigger asChild>
                                <button
                                    type="button"
                                    onClick={() => void submit()}
                                    disabled={!sendable}
                                    aria-label={sending ? '正在发送' : '发送'}
                                    className={cn(
                                        'mb-px inline-flex h-7 shrink-0 items-center gap-1 rounded-sm px-2.5 text-xs font-medium transition-colors',
                                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-1 focus-visible:ring-offset-canvas',
                                        sendable || sending
                                            ? 'bg-brand text-white hover:bg-brand-hover'
                                            : 'bg-inset text-text-disabled',
                                        'disabled:cursor-not-allowed',
                                    )}
                                >
                                    {sending ? (
                                        <Spinner size="xs" label="正在发送" className="text-white" />
                                    ) : (
                                        <SendHorizontal size={13} aria-hidden />
                                    )}
                                    <span className="hidden @min-[340px]/right:inline">发送</span>
                                </button>
                            </TooltipTrigger>
                            <TooltipContent side="top">{sending ? '正在发送…' : '发送（Enter）'}</TooltipContent>
                        </Tooltip>
                    </div>
                </PopoverAnchor>
                <PopoverContent
                    side="top"
                    align="start"
                    onOpenAutoFocus={(e) => e.preventDefault()}
                    onCloseAutoFocus={(e) => e.preventDefault()}
                    className="w-[270px] p-1"
                >
                    <MemberList
                        open={pickerOpen}
                        options={options}
                        activeIndex={activeIndex}
                        loading={contacts.isLoading}
                        error={contacts.error}
                        more={moreCount}
                        onPick={insertMention}
                        onHover={(index) => picker && setPicker({ ...picker, index })}
                    />
                </PopoverContent>
            </Popover>
            {error && (
                <div role="alert" className="mt-1 flex items-start gap-1.5 px-0.5 text-2xs leading-relaxed text-danger">
                    <TriangleAlert size={11} aria-hidden className="mt-0.5 shrink-0" />
                    <span className="min-w-0 flex-1 break-words">{error}</span>
                    <ChipClose label="关掉这条提示" onClick={() => setError(null)} className="hover:bg-danger-soft" />
                </div>
            )}
        </div>
    );
});

/** 小胶囊上的 ×：带悬停提示 */
function ChipClose({ label, onClick, className }: { label: string; onClick: () => void; className?: string }) {
    return (
        <IconAction
            label={label}
            onClick={onClick}
            onMouseDown={(e) => e.preventDefault()}
            className={cn('h-4 w-4 rounded-full text-current hover:bg-inset hover:text-current', className)}
        >
            <X size={10} aria-hidden />
        </IconAction>
    );
}

function dedupe(mentions: Mention[]): Mention[] {
    const seen = new Set<string>();
    const out: Mention[] = [];
    for (let i = mentions.length - 1; i >= 0; i -= 1) {
        const m = mentions[i] as Mention;
        const k = `${m.label}\u0001${m.qq}`;
        if (seen.has(k)) continue;
        seen.add(k);
        out.unshift(m);
    }
    return out;
}

function MemberList({
    open,
    options,
    activeIndex,
    loading,
    error,
    more,
    onPick,
    onHover,
}: {
    open: boolean;
    options: PickOption[];
    activeIndex: number;
    loading: boolean;
    error: string | null;
    more: number;
    onPick: (o: PickOption) => void;
    onHover: (index: number) => void;
}) {
    const listRef = useRef<HTMLDivElement>(null);
    useEffect(() => {
        listRef.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView?.({ block: 'nearest' });
    }, [activeIndex]);

    // 指针真的动了才跟着换高亮。弹层是在指针底下弹出来的：指针正好停在「全体成员」上时，
    // mouseenter（以及浏览器在布局变化、列表滚动后补发的 mousemove）都会落到它身上，
    // 这时按回车就一不小心 @ 了全体。所以只认坐标变了的 mousemove；每次打开从头算
    const lastPoint = useRef<{ x: number; y: number } | null>(null);
    useLayoutEffect(() => {
        if (open) lastPoint.current = null;
    }, [open]);
    const onRowMove = (index: number, e: ReactMouseEvent) => {
        const prev = lastPoint.current;
        lastPoint.current = { x: e.clientX, y: e.clientY };
        if (!prev || (prev.x === e.clientX && prev.y === e.clientY)) return;
        if (index !== activeIndex) onHover(index);
    };
    return (
        <div>
            <p className="px-2 pb-1 pt-0.5 text-2xs font-medium text-text-tertiary">@ 谁（↑↓ 选，回车确定）</p>
            <div ref={listRef} role="listbox" aria-label="群成员" className="max-h-[220px] overflow-y-auto">
                {options.map((o, i) => (
                    <div
                        key={`${o.qq}:${i}`}
                        role="option"
                        aria-selected={i === activeIndex}
                        onMouseDown={(e) => e.preventDefault()}
                        onMouseMove={(e) => onRowMove(i, e)}
                        onClick={() => onPick(o)}
                        className={cn(
                            'flex cursor-pointer items-baseline gap-2 rounded-xs px-2 py-1 text-xs',
                            i === activeIndex ? 'bg-brand-soft text-text' : 'text-text-secondary hover:bg-inset',
                        )}
                    >
                        <span className="min-w-0 truncate font-medium">{o.label}</span>
                        {o.hint && <span className="ml-auto min-w-0 shrink truncate text-2xs text-text-tertiary">{o.hint}</span>}
                    </div>
                ))}
            </div>
            {loading && (
                <p className="flex items-center gap-1.5 px-2 py-1.5 text-2xs text-text-tertiary">
                    <Spinner size="xs" label="正在拉群成员" />
                    正在拉群成员…
                </p>
            )}
            {error && (
                <p className="px-2 py-1.5 text-2xs leading-relaxed text-warning">拉不到群成员（{error}）。直接输入 QQ 号也能 @。</p>
            )}
            {more > 0 && <p className="px-2 pb-1 pt-0.5 text-2xs text-text-tertiary">还有 {more} 人，输入名字或 QQ 号缩小范围</p>}
            {!loading && !error && options.length === 0 && (
                <p className="px-2 py-1.5 text-2xs text-text-tertiary">没有叫这个名字的成员</p>
            )}
        </div>
    );
}
