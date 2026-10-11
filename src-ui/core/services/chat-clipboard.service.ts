import { invoke, isTauri } from '../ipc/transport';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import type { ChatFileSource } from '../ipc/generated/chat/ChatFileSource';
import type { Contact, Message } from '../domain/chat/model';
import { fileSegmentName, fileSegmentSource } from '../domain/chat/groupFiles';
import { messagePreview, segmentPreview } from '../domain/debug/segments';
import { markdownContent } from '../domain/debug/markdown';
import { LOCAL_FILE_PREFIX } from '../domain/debug/streamActions';
import { chatMediaService } from './chat-media.service';

type ClipboardMedia = {
    name: string;
    image: boolean;
    path?: string;
    url?: string;
    base64?: string;
    source?: ChatFileSource;
};
const string = (value: unknown) => (typeof value === 'string' ? value : '');
async function mediaReference(reference: string): Promise<Pick<ClipboardMedia, 'base64' | 'url'>> {
    if (reference.startsWith('blob:')) {
        const response = await fetch(reference);
        if (!response.ok) throw new Error('读取图片失败');
        const blob = await response.blob();
        if (blob.size > 64 * 1024 * 1024) throw new Error('媒体超过复制大小限制（64 MiB）');
        const data = await new Promise<string>((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () => resolve(String(reader.result));
            reader.onerror = () => reject(new Error('读取图片失败'));
            reader.readAsDataURL(blob);
        });
        return { base64: data.slice(data.indexOf(',') + 1) };
    }
    if (reference.startsWith('data:'))
        return { base64: reference.slice(reference.indexOf(',') + 1) };
    if (reference.startsWith('base64://')) return { base64: reference.slice(9) };
    if (/^https?:\/\//i.test(reference)) return { url: reference };
    throw new Error('媒体没有可复制的内容');
}

export async function copyChatMessage(
    target: DebugTarget,
    contact: Contact,
    message: Message,
): Promise<void> {
    const media: ClipboardMedia[] = [];
    const pieces: string[] = [];
    let visited = 0;
    const collect = async (
        segments: Message['segments'],
        messageId?: string,
        depth = 0,
    ): Promise<void> => {
        if (depth > 5) throw new Error('聊天记录嵌套过深，无法复制');
        let imageIndex = 0;
        for (const segment of segments) {
            if (++visited > 2000 || media.length >= 100) throw new Error('消息内容过大，无法复制');
            if (segment.type === 'forward') {
                const nodes = await chatMediaService.forward(target, segment.data);
                for (const node of nodes) {
                    pieces.push(`\n${node.name}：`);
                    await collect(node.segments, undefined, depth + 1);
                }
                continue;
            }
            pieces.push(
                segment.type === 'json' || segment.type === 'xml'
                    ? string(segment.data.data)
                    : segment.type === 'markdown'
                      ? markdownContent(segment.data)
                      : segmentPreview(segment),
            );
            if (segment.type === 'image' || segment.type === 'mface') {
                const reference = await chatMediaService.image(target, segment.data, false, {
                    context: { messageId, imageIndex: imageIndex++ },
                });
                media.push({
                    name: string(segment.data.name) || '图片.png',
                    image: true,
                    ...(await mediaReference(reference)),
                });
            } else if (segment.type === 'video' || segment.type === 'record') {
                const reference =
                    segment.type === 'video'
                        ? await chatMediaService.video(target, segment.data)
                        : await chatMediaService.record(target, segment.data);
                media.push({
                    name:
                        string(segment.data.name) ||
                        string(segment.data.file_name) ||
                        (segment.type === 'video'
                            ? reference.startsWith('data:video/webm')
                                ? '视频.webm'
                                : '视频.mp4'
                            : reference.startsWith('data:audio/wav')
                              ? '语音.wav'
                              : reference.startsWith('data:audio/ogg')
                                ? '语音.ogg'
                                : '语音.mp3'),
                    image: false,
                    ...(await mediaReference(reference)),
                });
            } else if (segment.type === 'file') {
                const path = string(segment.data.file);
                const source = fileSegmentSource(
                    {
                        ...segment.data,
                        file_id: segment.data.file_id ?? (depth === 0 ? message.fileId : undefined),
                    },
                    contact,
                );
                if (path.startsWith(LOCAL_FILE_PREFIX))
                    media.push({
                        name: fileSegmentName(segment.data),
                        image: false,
                        path: path.slice(LOCAL_FILE_PREFIX.length),
                    });
                else if (source)
                    media.push({ name: fileSegmentName(segment.data), image: false, source });
                else if (/^https?:\/\//i.test(string(segment.data.url) || path))
                    media.push({
                        name: fileSegmentName(segment.data),
                        image: false,
                        url: string(segment.data.url) || path,
                    });
                else throw new Error('这条文件消息缺少可下载的来源');
            }
        }
    };
    await collect(message.segments, message.id);
    const text = pieces.join('');
    if (isTauri) {
        await invoke('chat_copy_message', { botId: target.bot_id, text, media });
        return;
    }
    // Browser preview supports image + text through the standard Clipboard API.
    if (media.length) {
        if (media.length !== 1 || !media[0].image)
            throw new Error('文件和多媒体复制需要桌面客户端');
        const item = media[0];
        const response = await fetch(item.url || `data:image/png;base64,${item.base64}`);
        if (!response.ok) throw new Error('读取图片失败');
        const bitmap = await createImageBitmap(await response.blob());
        try {
            const canvas = document.createElement('canvas');
            canvas.width = bitmap.width;
            canvas.height = bitmap.height;
            const context = canvas.getContext('2d');
            if (!context) throw new Error('无法复制图片');
            context.drawImage(bitmap, 0, 0);
            const png = await new Promise<Blob>((resolve, reject) =>
                canvas.toBlob(
                    (value) => (value ? resolve(value) : reject(new Error('图片转换失败'))),
                    'image/png',
                ),
            );
            await navigator.clipboard.write([
                new ClipboardItem({
                    'image/png': png,
                    'text/plain': new Blob([text], { type: 'text/plain' }),
                }),
            ]);
        } finally {
            bitmap.close();
        }
    } else await navigator.clipboard.writeText(text || messagePreview(message.segments));
}
