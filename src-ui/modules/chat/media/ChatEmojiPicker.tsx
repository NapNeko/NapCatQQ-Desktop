// 收藏表情只在打开对应页时读取当前账号。
import { useEffect, useId, useState } from 'react';
import { QQFace, QQ_FACE_IDS } from './QQFace';
import { chatMediaService } from '../../../core/services/chat-media.service';
import { errorText } from '../../../core/domain/errors';
import type { Attachment } from '../../../core/domain/chat/model';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import './chat-media.css';
export function ChatEmojiPicker({ target, onSelect, disabledReason }: { target: DebugTarget; onSelect: (attachment: Attachment) => void; disabledReason: string }) {
    const [tab, setTab] = useState<'qq' | 'favorites'>('qq'); const panelId = useId();
    const [favorites, setFavorites] = useState<string[] | null>(null);
    const [error, setError] = useState(''); const [retry, setRetry] = useState(0);
    useEffect(() => {
        if (tab !== 'favorites' || disabledReason) return;
        let cancelled = false; setFavorites(null); setError('');
        void chatMediaService.favorites(target).then(value => { if (!cancelled) setFavorites(value); }).catch(e => { if (!cancelled) setError(errorText(e)); });
        return () => { cancelled = true; };
    }, [tab, target.bot_id, target.qq_id, disabledReason, retry]);
    return <div className="native-chat-face-picker">
        <div role="tablist" aria-label="表情分类" className="native-chat-face-tabs" onKeyDown={event => {
            if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
            event.preventDefault(); const next = tab === 'qq' ? 'favorites' : 'qq'; setTab(next);
            event.currentTarget.querySelector<HTMLButtonElement>('[data-tab=' + next + ']')?.focus();
        }}>{(['qq', 'favorites'] as const).map(value => <button key={value} data-tab={value} role="tab" id={panelId + '-' + value} aria-selected={tab === value} aria-controls={panelId} tabIndex={tab === value ? 0 : -1} onClick={() => setTab(value)}>{value === 'qq' ? 'QQ 表情' : '收藏表情'}</button>)}</div>
        <div role="tabpanel" id={panelId} aria-labelledby={panelId + '-' + tab} className="native-chat-face-panel">
            {tab === 'qq' ? <div className="native-chat-face-grid">{QQ_FACE_IDS.map(id => <button key={id} aria-label={'插入QQ 表情 ' + id} onClick={() => onSelect({ key: crypto.randomUUID(), type: 'face', id, name: 'QQ 表情 ' + id })}><QQFace id={id} size={28} /></button>)}</div>
                : disabledReason ? <p role="status">{disabledReason}</p> : error ? <div role="status"><p>{error}</p><button className="native-chat-media-retry" onClick={() => setRetry(value => value + 1)}>重试读取收藏表情</button></div>
                    : favorites === null ? <p role="status">正在读取收藏表情…</p> : !favorites.length ? <p role="status">当前账号没有收藏表情</p>
                        : <div className="native-chat-favorite-grid">{favorites.map((url, index) => <button key={url} aria-label={'插入收藏表情 ' + (index + 1)} onClick={() => onSelect({ key: crypto.randomUUID(), type: 'image', path: url, subType: 1, name: '收藏表情 ' + (index + 1) })}><img src={url} alt={'收藏表情 ' + (index + 1)} loading="lazy" referrerPolicy="no-referrer" /></button>)}</div>}
        </div>
    </div>;
}
