// 收发和选择器共用 QQ 表情资源规则。
import { useState } from 'react';
import { QQ_FACE_FALLBACK } from '../../../core/domain/chat/qqFaces';
const faceNames = new Map(QQ_FACE_FALLBACK.map((face) => [face.id, face.name]));
export function QQFace({
    id,
    size = 24,
    name,
    url,
}: {
    id: string;
    size?: number;
    name?: string;
    url?: string;
}) {
    const [failed, setFailed] = useState({ identity: '', attempts: 0 });
    const valid = /^\d{1,6}$/.test(id);
    const label = valid ? 'QQ 表情 ' + id : 'QQ 表情';
    const title = name || faceNames.get(id) || label;
    const sources = [
        ...new Set(
            [
                url && /^https?:\/\//i.test(url) ? url : '',
                `https://koishi.js.org/QFace/assets/qq_emoji/${id}/png/${id}.png`,
            ].filter(Boolean),
        ),
    ];
    const identity = JSON.stringify([id, url]);
    const attempts = failed.identity === identity ? failed.attempts : 0;
    if (!valid || attempts >= sources.length)
        return (
            <span
                className="mx-0.5 inline-block shrink-0 overflow-hidden align-middle text-2xs text-text-tertiary"
                style={{ width: size, height: size }}
                title={title}
            >
                {valid ? '表情 ' + id : '表情'}
            </span>
        );
    return (
        <img
            src={sources[attempts]}
            alt={label}
            title={title}
            width={size}
            height={size}
            loading="lazy"
            decoding="async"
            referrerPolicy="no-referrer"
            draggable={false}
            className="mx-0.5 inline-block shrink-0 align-middle object-contain"
            style={{ width: size, height: size }}
            onError={() => setFailed({ identity, attempts: attempts + 1 })}
        />
    );
}
