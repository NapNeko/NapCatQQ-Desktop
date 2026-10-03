// 长图保留可读宽度，缩放时保留光标下的图像位置。
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Maximize, MoveHorizontal, RotateCcw, X, ZoomIn, ZoomOut } from 'lucide-react';
import { Dialog, DialogContent, DialogTitle } from '../../shared/ui/Dialog';
import { imageScale, limitImageScale, zoomScroll, type ImageFit, type ImageSize } from '../../core/domain/chat/imageView';
import './chat-image-viewer.css';

export function ChatImageViewer({ src, onClose }: { src: string; onClose: () => void }) {
    return <Dialog open={!!src} onOpenChange={open => { if (!open) onClose(); }}>
        <DialogContent size="sheetWide" hideClose className="native-chat-image-dialog">
            {src && <ImageCanvas key={src} src={src} onClose={onClose} />}
        </DialogContent>
    </Dialog>;
}

function ImageCanvas({ src, onClose }: { src: string; onClose: () => void }) {
    const [viewport, setViewport] = useState<HTMLDivElement | null>(null);
    const [area, setArea] = useState<ImageSize>({ width: 0, height: 0 });
    const [natural, setNatural] = useState<ImageSize>({ width: 0, height: 0 });
    const [mode, setMode] = useState<ImageFit | 'manual'>('auto');
    const [manual, setManual] = useState(1);
    const [error, setError] = useState(false);
    const [attempt, setAttempt] = useState(0);
    const [dragging, setDragging] = useState(false);
    const pending = useRef<{ left: number; top: number } | null>(null);
    const drag = useRef<{ x: number; y: number; left: number; top: number; id: number } | null>(null);
    const ready = natural.width > 0 && !error;
    const scale = mode === 'manual' ? manual : imageScale(natural, area, mode);
    useEffect(() => {
        if (!viewport) return;
        const measure = () => setArea({ width: viewport.clientWidth, height: viewport.clientHeight });
        const observer = new ResizeObserver(measure);
        observer.observe(viewport); measure();
        return () => observer.disconnect();
    }, [viewport]);
    const zoom = useCallback((value: number, x = area.width / 2, y = area.height / 2) => {
        if (!viewport || !ready) return;
        const next = limitImageScale(value);
        pending.current = {
            left: zoomScroll(viewport.scrollLeft, x, area.width, natural.width, scale, next),
            top: zoomScroll(viewport.scrollTop, y, area.height, natural.height, scale, next),
        };
        setManual(next); setMode('manual');
    }, [viewport, ready, area, natural, scale]);
    useLayoutEffect(() => {
        if (!viewport || !pending.current) return;
        viewport.scrollLeft = pending.current.left; viewport.scrollTop = pending.current.top;
        pending.current = null;
    }, [viewport, scale, mode]);
    useEffect(() => {
        if (!viewport) return;
        const wheel = (event: WheelEvent) => {
            if (!ready) return;
            event.preventDefault();
            const rect = viewport.getBoundingClientRect();
            const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? area.height : 1);
            zoom(scale * Math.exp(-Math.max(-500, Math.min(500, delta)) * 0.002), event.clientX - rect.left, event.clientY - rect.top);
        };
        viewport.addEventListener('wheel', wheel, { passive: false });
        return () => viewport.removeEventListener('wheel', wheel);
    }, [viewport, ready, zoom, scale, area.height]);
    const fit = (next: ImageFit) => { pending.current = { left: 0, top: 0 }; setMode(next); if (viewport) { viewport.scrollLeft = 0; viewport.scrollTop = 0; } };
    return <div className="native-chat-image-viewer" onKeyDown={event => {
        if (event.key === '+' || event.key === '=') { event.preventDefault(); zoom(scale * 1.25); }
        if (event.key === '-') { event.preventDefault(); zoom(scale / 1.25); }
        if (event.key === '0') { event.preventDefault(); fit('fit'); }
    }}>
        <header><DialogTitle>图片</DialogTitle><span>{ready ? `${natural.width} × ${natural.height}` : ''}</span><button type="button" className="native-chat-icon" onClick={onClose} aria-label="关闭图片"><X size={18} /></button></header>
        <div ref={setViewport} className="native-chat-image-canvas" data-dragging={dragging} aria-label="图片画布，滚轮缩放，拖动查看" tabIndex={0}
            onPointerDown={event => {
                if (!ready || event.button !== 0 || event.pointerType === 'touch') return;
                event.preventDefault(); event.currentTarget.focus();
                drag.current = { x: event.clientX, y: event.clientY, left: event.currentTarget.scrollLeft, top: event.currentTarget.scrollTop, id: event.pointerId };
                event.currentTarget.setPointerCapture(event.pointerId); setDragging(true);
            }}
            onPointerMove={event => {
                const start = drag.current; if (!start || start.id !== event.pointerId) return;
                event.currentTarget.scrollLeft = start.left + start.x - event.clientX;
                event.currentTarget.scrollTop = start.top + start.y - event.clientY;
            }}
            onPointerUp={event => { if (drag.current?.id === event.pointerId) { drag.current = null; setDragging(false); event.currentTarget.releasePointerCapture(event.pointerId); } }}
            onLostPointerCapture={() => { drag.current = null; setDragging(false); }}
            onDoubleClick={() => mode === 'manual' && scale === 1 ? fit('fit') : zoom(1)}>
            {!ready && <div className="native-chat-image-state" role="status">{error ? <>图片加载失败<button type="button" onClick={() => { setError(false); setNatural({ width: 0, height: 0 }); setAttempt(value => value + 1); }}>重试</button></> : '正在加载图片…'}</div>}
            <div className="native-chat-image-stage" style={{ width: Math.max(area.width, natural.width * scale + 48), height: Math.max(area.height, natural.height * scale + 48), visibility: ready ? 'visible' : 'hidden' }}>
                <img key={attempt} src={src} alt="消息图片" draggable={false} onLoad={event => setNatural({ width: event.currentTarget.naturalWidth, height: event.currentTarget.naturalHeight })} onError={() => setError(true)} style={{ width: natural.width * scale || undefined, height: natural.height * scale || undefined, left: Math.max(24, (area.width - natural.width * scale) / 2), top: Math.max(24, (area.height - natural.height * scale) / 2) }} />
            </div>
        </div>
        <footer aria-label="图片缩放工具">
            <button type="button" disabled={!ready || scale <= 0.01} aria-label="缩小图片" title="缩小 · −" onClick={() => zoom(scale / 1.25)}><ZoomOut size={17} /></button>
            <button type="button" disabled={!ready} aria-label="原始尺寸" title="原始尺寸" onClick={() => zoom(1)} className="native-chat-image-percent">{Math.round(scale * 100)}%</button>
            <button type="button" disabled={!ready || scale >= 8} aria-label="放大图片" title="放大 · +" onClick={() => zoom(scale * 1.25)}><ZoomIn size={17} /></button>
            <span className="native-chat-image-toolbar-divider" />
            <button type="button" disabled={!ready} aria-label="适应窗口" title="适应窗口 · 0" onClick={() => fit('fit')}><Maximize size={16} /><span>适应</span></button>
            <button type="button" disabled={!ready} aria-label="适应宽度" title="适应宽度" onClick={() => fit('width')}><MoveHorizontal size={17} /><span>宽度</span></button>
            <button type="button" disabled={!ready} aria-label="重置图片视图" title="重置" onClick={() => fit('auto')}><RotateCcw size={15} /></button>
        </footer>
    </div>;
}
