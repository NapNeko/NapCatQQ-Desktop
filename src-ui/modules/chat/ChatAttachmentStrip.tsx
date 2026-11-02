// 附件预览保留退出中的条目，让草稿编辑与预览资源的生命周期各自独立。
import { memo, useEffect, useMemo, useRef, useState } from 'react';
import { File, ImagePlus, X } from 'lucide-react';
import type { Attachment } from '../../core/domain/chat/model';
import { QQFace } from '../../shared/chat/media/QQFace';
import { Button } from '../../shared/ui/Button';
import { ChatPresence } from './chatMotion';

export function ChatAttachmentStrip({
    attachments,
    onRemove,
    readLocalImage,
}: {
    attachments: Attachment[];
    onRemove: (key: string) => void;
    readLocalImage: (path: string) => Promise<string>;
}) {
    const [retained, setRetained] = useState(attachments);
    const currentKeys = useMemo(() => new Set(attachments.map((file) => file.key)), [attachments]);
    const latestKeys = useRef(currentKeys);
    latestKeys.current = currentKeys;
    const displayed = useMemo(
        () => [...attachments, ...retained.filter((file) => !currentKeys.has(file.key))],
        [attachments, retained, currentKeys],
    );
    useEffect(() => {
        if (!attachments.length) return;
        setRetained((previous) => {
            const files = new Map(previous.map((file) => [file.key, file]));
            for (const file of attachments) files.set(file.key, file);
            return [...files.values()];
        });
    }, [attachments]);
    if (!displayed.length) return null;
    return (
        <div className="native-chat-attachments" role="list" aria-label="待发送附件">
            {displayed.map((file) => (
                <ChatPresence
                    key={file.key}
                    visible={currentKeys.has(file.key)}
                    onExited={() => {
                        if (!latestKeys.current.has(file.key))
                            setRetained((previous) =>
                                previous.filter((item) => item.key !== file.key),
                            );
                    }}
                >
                    <div className="native-chat-attachment" data-type={file.type} role="listitem">
                        <div className="native-chat-attachment-preview">
                            <AttachmentPreview attachment={file} readLocalImage={readLocalImage} />
                        </div>
                        <span className="native-chat-attachment-name" title={file.name}>
                            {file.name}
                        </span>
                        <Button
                            variant="ghost"
                            size="icon"
                            className="native-chat-attachment-remove"
                            aria-label={`移除${file.name}`}
                            title={`移除${file.name}`}
                            onClick={() => onRemove(file.key)}
                        >
                            <X size={12} />
                        </Button>
                    </div>
                </ChatPresence>
            ))}
        </div>
    );
}

const AttachmentPreview = memo(function AttachmentPreview({
    attachment,
    readLocalImage,
}: {
    attachment: Attachment;
    readLocalImage: (path: string) => Promise<string>;
}) {
    const [failed, setFailed] = useState('');
    const [preview, setPreview] = useState({ path: '', source: '' });
    const previewPath = attachment.type === 'image' ? attachment.previewPath : undefined;
    useEffect(() => {
        if (!previewPath) return;
        let disposed = false;
        void readLocalImage(previewPath)
            .then((bytes) => {
                if (!disposed)
                    setPreview({ path: previewPath, source: `data:image/png;base64,${bytes}` });
            })
            .catch(() => {});
        return () => {
            disposed = true;
        };
    }, [previewPath, readLocalImage]);
    if (attachment.type === 'face') return <QQFace id={attachment.id} size={40} />;
    if (attachment.type === 'file') return <File size={28} strokeWidth={1.5} />;
    const source =
        previewPath && preview.path === previewPath
            ? preview.source
            : attachment.path.startsWith('base64://')
              ? 'data:image/png;base64,' + attachment.path.slice(9)
              : /^https?:\/\//i.test(attachment.path)
                ? attachment.path
                : '';
    return source && attachment.path !== failed ? (
        <img
            src={source}
            alt={attachment.name}
            referrerPolicy="no-referrer"
            decoding="async"
            draggable={false}
            onError={() => setFailed(attachment.path)}
        />
    ) : (
        <ImagePlus size={28} strokeWidth={1.5} />
    );
});
