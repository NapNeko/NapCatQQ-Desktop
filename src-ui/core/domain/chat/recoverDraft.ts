// 从发送记录恢复可编辑内容；本机附件路径只存在本次会话内存中。
import { LOCAL_FILE_PREFIX } from '../debug/streamActions';
import { text, type Draft, type Message } from './model';

export function recoverDraft(message: Message, current: Draft): Draft {
    const attachments = [...current.attachments];
    const mentions = [...(current.mentions ?? [])];
    const parts: string[] = [];
    let reply = current.reply;
    message.segments.forEach((segment, index) => {
        const data = segment.data;
        if (segment.type === 'text') parts.push(text(data.text));
        else if (segment.type === 'at') {
            const qq = String(data.qq); const label = `@${qq}`;
            mentions.push({ qq, label }); parts.push(label);
        } else if (segment.type === 'reply') {
            reply = { id: String(data.id), name: '原消息', preview: `#${String(data.id)}` };
        } else if (segment.type === 'face' && /^\d{1,6}$/.test(String(data.id))) {
            attachments.push({ key: message.key + '/' + index + '/' + attachments.length, type: 'face', id: String(data.id), name: 'QQ 表情 ' + String(data.id) });
        } else if (segment.type === 'image' && /^https?:\/\//i.test(text(data.file))) {
            attachments.push({ key: message.key + '/' + index + '/' + attachments.length, type: 'image', path: text(data.file), name: data.sub_type === 1 ? '收藏表情' : '图片', ...(data.sub_type === 1 ? { subType: 1 } : {}) });
        } else if (segment.type === 'image' || segment.type === 'file') {
            const file = text(data.file);
            if (file.startsWith(LOCAL_FILE_PREFIX) || file.startsWith('base64://')) {
                const path = file.startsWith(LOCAL_FILE_PREFIX) ? file.slice(LOCAL_FILE_PREFIX.length) : file;
                attachments.push({ key: `${message.key}/${index}/${attachments.length}`, type: segment.type, path, name: text(data.name) || (path.startsWith('base64://') ? '粘贴的图片.png' : path.split(/[\\/]/).pop() || '附件') });
            }
        }
    });
    return { text: [current.text, parts.join('')].filter(Boolean).join('\n'), mentions, attachments, reply };
}
