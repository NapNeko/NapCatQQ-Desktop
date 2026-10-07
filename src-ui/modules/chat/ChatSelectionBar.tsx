// 多选操作浮在会话区域内，外层定位与内层动效分开。
import { Forward, X } from 'lucide-react';
import gsap from 'gsap';
import { Button } from '../../shared/ui/Button';
import { Counter, GsapPresence, type EnterFn, type ExitFn } from '../../shared/ui/motion';
import { FORWARD_MESSAGE_LIMIT } from '../../core/domain/chat/messageTransfer';

const enter: EnterFn = (element, env) =>
    gsap.fromTo(
        element,
        { autoAlpha: 0, y: 18, scale: 0.96 },
        { autoAlpha: 1, y: 0, scale: 1, duration: env.duration('base'), ease: env.ease.release },
    );
const exit: ExitFn = (element, env) =>
    gsap.to(element, {
        autoAlpha: 0,
        y: 14,
        scale: 0.96,
        duration: env.duration('fast'),
        ease: env.ease.exit,
    });

export function ChatSelectionBar({
    visible,
    count,
    disabled,
    onCancel,
    onForward,
}: {
    visible: boolean;
    count: number;
    disabled: boolean;
    onCancel: () => void;
    onForward: () => void;
}) {
    return (
        <div className="native-chat-selection-dock">
            <GsapPresence visible={visible} onEnter={enter} onExit={exit}>
                <div
                    className="native-chat-selection-bar shadow-popover"
                    role="toolbar"
                    aria-label="消息多选"
                    style={{ visibility: 'hidden', opacity: 0 }}
                >
                    <span className="native-chat-selection-count" aria-live="polite">
                        已选 <Counter value={count} /> / {FORWARD_MESSAGE_LIMIT}
                    </span>
                    <span className="native-chat-selection-divider" aria-hidden />
                    <Button
                        variant="primary"
                        size="sm"
                        disabled={!count || disabled}
                        onClick={onForward}
                    >
                        <Forward size={14} />
                        转发
                    </Button>
                    <Button variant="ghost" size="sm" title="取消多选 · Esc" onClick={onCancel}>
                        <X size={14} />
                        取消
                    </Button>
                </div>
            </GsapPresence>
        </div>
    );
}
