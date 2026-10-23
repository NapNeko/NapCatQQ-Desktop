// Bot 头像：按 QQ 号拉取头像，失败或未填时回退首字母渐变块，右下角带底座色点。

import { useState } from 'react';
import { Bot } from 'lucide-react';
import { cn } from '../../shared/utils/cn';
import { useAvatarUrl } from '../../hooks/ui/useAvatarUrl';

const PALETTES = [
    'from-pink-300 to-rose-400',
    'from-amber-300 to-orange-400',
    'from-emerald-300 to-teal-400',
    'from-sky-300 to-indigo-400',
    'from-violet-300 to-fuchsia-400',
    'from-rose-300 to-red-400',
] as const;

function pickAvatarPalette(seed: string): string {
    let h = 0;
    for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) | 0;
    return PALETTES[Math.abs(h) % PALETTES.length] ?? PALETTES[0];
}

export function BotAvatar({
    qqid,
    displayName,
    flavorTone,
    size = 'md',
}: {
    qqid: string;
    displayName: string;
    flavorTone: 'brand' | 'info';
    size?: 'sm' | 'md';
}) {
    const [failed, setFailed] = useState<string | null>(null);
    const trimmedQq = qqid.trim();
    const numericQQ = /^[1-9]\d{4,11}$/.test(trimmedQq) ? trimmedQq : null;

    const url = useAvatarUrl(
        numericQQ ? `https://q.qlogo.cn/headimg_dl?dst_uin=${numericQQ}&spec=640` : null,
    );
    const showImg = url && failed !== url;
    const trimmedName = displayName.trim();
    const initials = trimmedName ? trimmedName.charAt(0).toUpperCase() : null;
    const palette = pickAvatarPalette(trimmedQq || trimmedName || 'bot');

    const sizeCls = size === 'sm' ? 'h-9 w-9' : 'h-11 w-11';
    const dotCls = size === 'sm' ? 'h-2 w-2' : 'h-2.5 w-2.5';

    return (
        <div className={cn('relative shrink-0', sizeCls)}>
            <div
                className={cn(
                    'h-full w-full overflow-hidden rounded-md ring-1 ring-border-subtle',
                    'bg-gradient-to-br',
                    palette,
                )}
            >
                {showImg && (
                    <img
                        src={url}
                        alt=""
                        className="h-full w-full object-cover"
                        referrerPolicy="no-referrer"
                        draggable={false}
                        onError={() => setFailed(url)}
                    />
                )}
                {!showImg && (
                    <div className="flex h-full w-full items-center justify-center font-display text-sm font-semibold text-white/95">
                        {initials ?? <Bot size={size === 'sm' ? 16 : 18} />}
                    </div>
                )}
            </div>
            <span
                aria-hidden
                className={cn(
                    'absolute -bottom-0.5 -right-0.5 inline-block rounded-full ring-2 ring-elevated',
                    dotCls,
                    flavorTone === 'info' ? 'bg-info' : 'bg-brand',
                )}
            />
        </div>
    );
}
