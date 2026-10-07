// 收发和选择器共用 QQ 表情资源与本地缓存规则。
import { useEffect, useState } from 'react';
import { qqFaceLarge, QQ_FACE_FALLBACK } from '../../../core/domain/chat/qqFaces';
import { qqFaceAssetService } from '../../../core/services/qq-face-assets.service';
import { qqFaceService } from '../../../core/services/qq-face.service';
import { bundledQQFaceSource } from '../../../core/services/qq-face-bundled.service';
import { useMotion } from '../../../hooks/preferences/useMotion';
const faceNames = new Map(QQ_FACE_FALLBACK.map((face) => [face.id, face.name]));
export function qqFaceSources(id: string, animated: boolean, url?: string): string[] {
    const path = `assets/qq_emoji/${id}`;
    const origins = [
        'https://koishi.js.org/QFace',
        'https://cdn.jsdelivr.net/gh/koishijs/QFace@master/public',
    ];
    const local = bundledQQFaceSource(id);
    return [
        ...new Set(
            [
                ...(animated ? origins.map((origin) => `${origin}/${path}/apng/${id}.png`) : []),
                local,
                url && /^https?:\/\//i.test(url) ? url : '',
                ...origins.map((origin) => `${origin}/${path}/png/${id}.png`),
            ].filter(Boolean),
        ),
    ];
}
export function QQFace({
    id,
    size,
    name,
    url,
    animated = false,
    data,
    displayLarge,
}: {
    id: string;
    size?: number;
    name?: string;
    url?: string;
    animated?: boolean;
    data?: Record<string, unknown>;
    displayLarge?: boolean;
}) {
    const motion = useMotion();
    const [failed, setFailed] = useState({ identity: '', attempts: 0 });
    const [loaded, setLoaded] = useState({ source: '', url: '' });
    const valid = /^(0|[1-9]\d{0,5})$/.test(id);
    const superFace = qqFaceLarge(data) ?? displayLarge ?? false;
    const displaySize = size ?? (animated && superFace ? 72 : 24);
    const label = valid ? 'QQ 表情 ' + id : 'QQ 表情';
    const title = name || qqFaceService.peek(id)?.name || faceNames.get(id) || label;
    const sources = qqFaceSources(id, animated && superFace && motion.enabled, url);
    const identity = JSON.stringify(sources);
    const attempts = failed.identity === identity ? failed.attempts : 0;
    const source = valid ? sources[attempts] : undefined;
    const local = valid ? bundledQQFaceSource(id) : '';
    useEffect(() => {
        if (!source || source === local) return;
        let cancelled = false;
        let release: (() => void) | undefined;
        void qqFaceAssetService
            .acquire(source)
            .then((lease) => {
                if (cancelled) {
                    lease.release();
                    return;
                }
                release = lease.release;
                setLoaded({ source, url: lease.url });
            })
            .catch((error: unknown) => {
                if (cancelled) return;
                if (
                    typeof error === 'object' &&
                    error !== null &&
                    'name' in error &&
                    error.name === 'AbortError'
                )
                    setFailed({ identity, attempts: attempts + 1 });
                else setLoaded({ source, url: source });
            });
        return () => {
            cancelled = true;
            release?.();
        };
    }, [source, identity, attempts, local]);
    if (!source)
        return (
            <span
                className="mx-0.5 inline-block shrink-0 overflow-hidden align-middle text-2xs text-text-tertiary"
                style={{ width: displaySize, height: displaySize }}
                title={title}
            >
                {valid ? '表情 ' + id : '表情'}
            </span>
        );
    return (
        <img
            src={source === local ? local : loaded.source === source ? loaded.url : local || source}
            alt={label}
            title={title}
            width={displaySize}
            height={displaySize}
            loading="lazy"
            decoding="async"
            referrerPolicy="no-referrer"
            draggable={false}
            className="mx-0.5 inline-block shrink-0 align-middle object-contain"
            style={{ width: displaySize, height: displaySize }}
            onError={() => {
                void qqFaceAssetService.invalidate(source).catch(() => {});
                setFailed({ identity, attempts: attempts + 1 });
            }}
        />
    );
}
