// 试聊的输入框：回车发、Shift + 回车换行，输入法选字时的回车不算。图片能粘贴、能挑、能拖进窗口；
// 上游一条最多收 8 张，本机图片预览超过 3 MB 就不给读，这里同样按 3 MB 卡。

import { useRef, useState } from 'react';
import { ImagePlus, SendHorizontal, X } from 'lucide-react';
import { Button } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import {
    CHAT_IMAGE_MAX,
    splitDataUrl,
    type MaiBotChatImage,
} from '../../../../core/domain/apps/maibotChat';
import { errorText } from '../../../../core/domain/errors';
import type { MaiBotLocalImage } from '../../../../core/ipc/types';
import { useMaiBotChatImages } from '../../../../hooks/apps/useMaiBotChat';
import { useTauriFileDrop } from '../../../../hooks/ui/useTauriFileDrop';

type Attachment = { key: string; name: string; src?: string; problem?: string };

const PASTE_TYPES = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp']);
const IMAGE_MAX_BYTES = 3 * 1024 * 1024;
const AREA_MAX_PX = 160;

function readPasted(file: File, key: string): Promise<Attachment> {
    const name = file.name || '粘贴的图片';
    if (!PASTE_TYPES.has(file.type))
        return Promise.resolve({ key, name, problem: '只能发 PNG / JPG / GIF / WebP' });
    if (file.size > IMAGE_MAX_BYTES)
        return Promise.resolve({ key, name, problem: '超过 3 MB，发不了' });
    return new Promise((resolve) => {
        const r = new FileReader();
        r.onload = () =>
            resolve({ key, name, src: typeof r.result === 'string' ? r.result : undefined });
        r.onerror = () => resolve({ key, name, problem: '读不出这张图' });
        r.readAsDataURL(file);
    });
}

export const ChatComposer: React.FC<{
    ready: boolean;
    placeholder: string;
    onSend: (text: string, images: MaiBotChatImage[]) => Promise<void>;
}> = ({ ready, placeholder, onSend }) => {
    const [text, setText] = useState('');
    const [atts, setAtts] = useState<Attachment[]>([]);
    const [sending, setSending] = useState(false);
    const [hint, setHint] = useState<{ text: string; error: boolean } | null>(null);
    const area = useRef<HTMLTextAreaElement>(null);
    const seq = useRef(0);
    const nextKey = () => `a${++seq.current}`;
    // 读图是异步的，回来时按当下已有几张算还能加几张
    const count = useRef(0);
    count.current = atts.length;

    const add = (list: Attachment[]) => {
        const room = Math.max(0, CHAT_IMAGE_MAX - count.current);
        const kept = list.slice(0, room);
        count.current += kept.length;
        setHint(
            list.length > room
                ? { text: `一次最多发 ${CHAT_IMAGE_MAX} 张，多的没加上`, error: false }
                : null,
        );
        setAtts((prev) => [...prev, ...kept].slice(0, CHAT_IMAGE_MAX));
    };
    const addLocal = (imgs: MaiBotLocalImage[]) =>
        add(
            imgs.map((i) => ({
                key: nextKey(),
                name: i.name,
                src: i.problem ? undefined : i.preview,
                // 后端超过 3 MB 就不给预览，预览正是要发出去的那份
                problem: i.problem ?? (i.preview ? undefined : '超过 3 MB，发不了'),
            })),
        );

    const images = useMaiBotChatImages();
    const { dragging } = useTauriFileDrop(ready, (paths) => void images.read(paths).then(addLocal));

    const fit = () => {
        const el = area.current;
        if (!el) return;
        el.style.height = 'auto';
        el.style.height = `${Math.min(el.scrollHeight, AREA_MAX_PX)}px`;
    };

    const usable = atts.filter((a) => a.src && !a.problem);
    const canSend = ready && !sending && (text.trim().length > 0 || usable.length > 0);

    const submit = async () => {
        if (!canSend) return;
        const images = usable.flatMap((a) => {
            const parts = a.src ? splitDataUrl(a.src) : null;
            return parts ? [{ name: a.name, ...parts }] : [];
        });
        setSending(true);
        try {
            await onSend(text.trim(), images);
            setText('');
            setAtts([]);
            setHint(null);
            requestAnimationFrame(fit);
        } catch (err) {
            setHint({ text: `没发出去：${errorText(err)}`, error: true });
        } finally {
            setSending(false);
            area.current?.focus();
        }
    };

    return (
        <div
            className={cn(
                'rounded-lg border bg-surface px-2 pb-2 pt-1.5 transition-colors',
                dragging
                    ? 'border-brand/60 bg-brand-soft/20'
                    : 'border-border-subtle focus-within:border-brand/40',
            )}
        >
            {atts.length > 0 && (
                <div className="flex flex-wrap gap-2 px-1 pb-2 pt-1">
                    {atts.map((a) => (
                        <span
                            key={a.key}
                            title={a.problem ? `${a.name}：${a.problem}` : a.name}
                            className={cn(
                                'group relative h-14 w-14 overflow-hidden rounded-md ring-1',
                                a.problem ? 'ring-danger/60' : 'ring-border-subtle',
                            )}
                        >
                            {a.src ? (
                                <img
                                    src={a.src}
                                    alt={a.name}
                                    className="h-full w-full object-cover"
                                    draggable={false}
                                />
                            ) : (
                                <span className="flex h-full w-full items-center justify-center bg-danger-soft p-1 text-center text-[10px] leading-tight text-danger">
                                    {a.problem}
                                </span>
                            )}
                            <button
                                type="button"
                                aria-label="不发这张"
                                onClick={() =>
                                    setAtts((prev) => prev.filter((x) => x.key !== a.key))
                                }
                                className="absolute right-0.5 top-0.5 hidden h-4 w-4 items-center justify-center rounded-full bg-black/55 text-white group-hover:flex"
                            >
                                <X size={10} />
                            </button>
                        </span>
                    ))}
                </div>
            )}
            <div className="flex items-end gap-1.5">
                <Button
                    size="icon"
                    variant="ghost"
                    className="h-8 w-8 shrink-0"
                    aria-label="发图片"
                    title="发图片，也可以直接粘贴或拖进窗口"
                    disabled={!ready || atts.length >= CHAT_IMAGE_MAX}
                    onClick={() => void images.pick().then(addLocal)}
                >
                    <ImagePlus size={16} />
                </Button>
                <textarea
                    ref={area}
                    rows={1}
                    value={text}
                    disabled={!ready}
                    placeholder={placeholder}
                    aria-label="消息"
                    onChange={(e) => {
                        setText(e.target.value);
                        fit();
                    }}
                    onKeyDown={(e) => {
                        // 229 是部分输入法选字时报的键码，isComposing 在老 WebView 上不一定准
                        if (
                            e.key === 'Enter' &&
                            !e.shiftKey &&
                            !e.nativeEvent.isComposing &&
                            e.keyCode !== 229
                        ) {
                            e.preventDefault();
                            void submit();
                        }
                    }}
                    onPaste={(e) => {
                        const files = [...e.clipboardData.files].filter((f) =>
                            f.type.startsWith('image/'),
                        );
                        if (files.length === 0) return;
                        e.preventDefault();
                        void Promise.all(files.map((f) => readPasted(f, nextKey()))).then(add);
                    }}
                    className="max-h-40 min-h-8 flex-1 resize-none bg-transparent py-1.5 text-[13.5px] leading-relaxed text-text outline-none placeholder:text-text-tertiary disabled:cursor-not-allowed"
                />
                <Button
                    size="icon"
                    className="h-8 w-8 shrink-0"
                    aria-label="发送"
                    disabled={!canSend}
                    onClick={() => void submit()}
                >
                    <SendHorizontal size={15} />
                </Button>
            </div>
            {hint && (
                <p
                    className={cn(
                        'px-1 pt-1 text-2xs',
                        hint.error ? 'text-danger' : 'text-text-tertiary',
                    )}
                >
                    {hint.text}
                </p>
            )}
        </div>
    );
};
