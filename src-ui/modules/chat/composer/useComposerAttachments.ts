// 文件选择、原生拖放和浏览器粘贴共用附件入口；只管理文件读取与草稿追加。
import {
    useCallback,
    useEffect,
    useRef,
    useState,
    type ClipboardEvent,
    type DragEvent,
    type RefObject,
} from 'react';
import type { Attachment } from '../../../core/domain/chat/model';
import { errorText } from '../../../core/domain/errors';
import type { ChatAccountStore } from '../../../hooks/chat/chatStore';
import { useChatSend } from '../../../hooks/chat/useChatSend';
import type { ComposerDraftAccess } from './useComposerDraft';

const ATTACHMENT_LIMIT = 8;
const LIMIT_MESSAGE = '一次最多添加 8 个附件';
const IMAGE_EXTENSION = /\.(png|jpe?g|gif|webp|bmp)$/i;

function readBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => {
            const data = String(reader.result);
            resolve(data.slice(data.indexOf(',') + 1));
        };
        reader.onerror = () => reject(new Error('读取图片失败'));
        reader.readAsDataURL(file);
    });
}

// 普通会话切换保留原草稿目标；窗口交接或账号释放后，迟到结果不能再写旧视图。
function attachmentOwner(store: ChatAccountStore) {
    const revision = store.viewRevision;
    const ownerRevision = store.ownerRevision;
    const selfId = store.getSnapshot().account.selfId;
    return () =>
        !store.releaseWhenIdle &&
        store.viewRevision === revision &&
        store.ownerRevision === ownerRevision &&
        store.getSnapshot().account.selfId === selfId;
}

export function useComposerAttachments({
    store,
    editor,
    composer,
    input,
    collapsed,
    onError,
}: {
    store: ChatAccountStore;
    editor: ComposerDraftAccess;
    composer: RefObject<HTMLDivElement>;
    input: RefObject<HTMLTextAreaElement>;
    collapsed: boolean;
    onError: (message: string) => void;
}) {
    const { pickFile, readLocalImage, onFileDragDrop, stageAttachment } = useChatSend(store);
    const [pending, setPending] = useState(0);
    const [dragging, setDragging] = useState(false);
    const dragDepth = useRef(0);
    const dropEnabled = useRef(!collapsed);
    dropEnabled.current = !collapsed;
    const append = useCallback(
        (files: Attachment[]) => {
            const attachments = editor.read().attachments;
            const slots = Math.max(0, ATTACHMENT_LIMIT - attachments.length);
            if (files.length > slots) onError(LIMIT_MESSAGE);
            if (!slots) return false;
            editor.patch({ attachments: [...attachments, ...files.slice(0, slots)] });
            return true;
        },
        [editor, onError],
    );
    useEffect(() => {
        if (collapsed) {
            setDragging(false);
            dragDepth.current = 0;
        }
    }, [collapsed]);
    useEffect(() => {
        if (!onFileDragDrop) return;
        let alive = true;
        const listening = onFileDragDrop((event) => {
            if (!alive) return;
            if (event.type === 'leave' || !dropEnabled.current) {
                setDragging(false);
                return;
            }
            const bounds = composer.current?.getBoundingClientRect();
            const point = event.position;
            const inside =
                !!bounds &&
                !!point &&
                point.x >= bounds.left &&
                point.x <= bounds.right &&
                point.y >= bounds.top &&
                point.y <= bounds.bottom;
            setDragging(inside && event.type !== 'drop');
            if (event.type !== 'drop' || !inside) return;
            const slots = Math.max(0, ATTACHMENT_LIMIT - editor.read().attachments.length);
            onError(event.paths.length > slots ? LIMIT_MESSAGE : '');
            append(
                event.paths.slice(0, slots).map((path): Attachment => ({
                    key: crypto.randomUUID(),
                    path,
                    name: path.split(/[\\/]/).pop() || path,
                    type: IMAGE_EXTENSION.test(path) ? 'image' : 'file',
                })),
            );
            input.current?.focus({ preventScroll: true });
        });
        void listening.catch((error) => {
            if (alive) onError(errorText(error));
        });
        return () => {
            alive = false;
            void listening.then((unlisten) => unlisten()).catch(() => {});
        };
    }, [onFileDragDrop, composer, input, append, editor, onError]);
    const pick = async (type: 'image' | 'file') => {
        const isCurrent = attachmentOwner(store);
        onError('');
        setPending((count) => count + 1);
        try {
            const file = await pickFile();
            if (!isCurrent() || !file) return;
            if (type === 'image' && !IMAGE_EXTENSION.test(file.name))
                throw new Error('请选择 PNG、JPG、GIF、WebP 或 BMP 图片');
            append([{ ...file, key: crypto.randomUUID(), type }]);
        } catch (error) {
            if (isCurrent()) onError(errorText(error));
        } finally {
            setPending((count) => count - 1);
        }
    };
    const addFiles = async (files: File[]) => {
        const isCurrent = attachmentOwner(store);
        const slots = Math.max(0, ATTACHMENT_LIMIT - editor.read().attachments.length);
        const overflow = files.length > slots;
        onError(overflow ? LIMIT_MESSAGE : '');
        // 顺序读取，避免多份大附件同时膨胀成 base64 驻留内存。
        for (const file of files.slice(0, slots)) {
            if (!isCurrent()) return;
            setPending((count) => count + 1);
            try {
                const image = file.type.startsWith('image/');
                if (file.size > (image ? 10 : 64) * 1024 * 1024)
                    throw new Error(
                        image
                            ? '粘贴图片不能超过 10 MB'
                            : '浏览器附件不能超过 64 MB，请用附件按钮选择',
                    );
                const name = file.name || '粘贴的图片.png';
                const base64 = await readBase64(file);
                if (!isCurrent()) return;
                const path = await stageAttachment(name, base64);
                if (!isCurrent()) return;
                append([{ key: crypto.randomUUID(), path, name, type: image ? 'image' : 'file' }]);
            } catch (error) {
                if (isCurrent()) onError(errorText(error));
            } finally {
                setPending((count) => count - 1);
            }
        }
    };
    return {
        pending,
        dragging,
        readLocalImage,
        pick,
        add: (attachment: Attachment) => {
            if (!append([attachment])) return false;
            onError('');
            return true;
        },
        remove: (key: string) =>
            editor.patch({
                attachments: editor.read().attachments.filter((file) => file.key !== key),
            }),
        handlers: {
            onPaste: (event: ClipboardEvent) => {
                if (collapsed || !event.clipboardData.files.length) return;
                event.preventDefault();
                void addFiles(Array.from(event.clipboardData.files));
            },
            onDragOver: (event: DragEvent) => event.preventDefault(),
            onDrop: (event: DragEvent) => {
                event.preventDefault();
                dragDepth.current = 0;
                setDragging(false);
                if (!collapsed && !onFileDragDrop)
                    void addFiles(Array.from(event.dataTransfer.files));
            },
            onDragEnter: (event: DragEvent) => {
                if (!collapsed && Array.from(event.dataTransfer.types).includes('Files')) {
                    dragDepth.current++;
                    setDragging(true);
                }
            },
            onDragLeave: () => {
                dragDepth.current = Math.max(0, dragDepth.current - 1);
                if (!dragDepth.current) setDragging(false);
            },
        },
    };
}
