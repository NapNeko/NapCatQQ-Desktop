// 收发和选择器共用 QQ 表情资源规则。
import { useState } from 'react';
export const QQ_FACE_IDS = [...Array.from({ length: 40 }, (_, i) => i).filter(id => id !== 17), 49, 53, 60, 63, 66, 74, 75, 76, 78, 79, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 114, 116, 118, 119, 120, 121, 124, 137, 146, 172, 173, 174, 175, 176, 177, 178, 179, 181, 182, 183, 212, 262, 263, 264, 265, 266, 267, 268, 269, 270, 271, 272, 277, 281, 282, 283, 284, 285, 293, 294, 297, 298, 299, 300, 305, 306, 307, 311, 312, 314, 317, 318, 319, 320, 324, 325, 326, 337, 338, 339, 341, 342, 343, 344, 345, 346].map(String);
export function QQFace({ id, size = 24 }: { id: string; size?: number }) {
    const [failedId, setFailedId] = useState<string | null>(null);
    const valid = /^\d{1,6}$/.test(id); const label = valid ? 'QQ 表情 ' + id : 'QQ 表情';
    if (!valid || failedId === id) return <span className="text-2xs text-text-tertiary" title={label}>{valid ? '表情 ' + id : '表情'}</span>;
    return <img src={'https://koishi.js.org/QFace/assets/qq_emoji/' + id + '/png/' + id + '.png'} alt={label} title={label} width={size} height={size} loading="lazy" decoding="async" referrerPolicy="no-referrer" draggable={false} className="mx-0.5 inline-block shrink-0 align-middle object-contain" style={{ width: size, height: size }} onError={() => setFailedId(id)} />;
}
