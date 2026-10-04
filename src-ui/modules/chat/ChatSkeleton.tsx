// 首屏骨架：会话/消息还没读到时的占位。色块静态，rich 档整行轻脉冲（chat.css 按 data-motion 门控）。
import { cn } from '../../shared/utils/cn';

const CONVERSATION_ROWS = [82, 64, 74, 56, 70, 60];
const MESSAGE_ROWS: ReadonlyArray<{ mine: boolean; width: number }> = [
    { mine: false, width: 42 },
    { mine: true, width: 30 },
    { mine: false, width: 56 },
    { mine: false, width: 34 },
    { mine: true, width: 48 },
];

export function ConversationSkeleton() {
    return <div className="native-chat-skel-list" role="status" aria-label="正在读取会话">
        {CONVERSATION_ROWS.map((width, index) => (
            <div className="native-chat-skel" key={index} aria-hidden>
                <span className="native-chat-skel-avatar is-list" />
                <div className="min-w-0 flex-1">
                    <span className="native-chat-skel-bar" style={{ width: `${width}%` }} />
                    <span className="native-chat-skel-bar mt-2" style={{ width: `${Math.round(width * 0.62)}%` }} />
                </div>
            </div>))}
    </div>;
}

export function MessageSkeleton() {
    return <div className="native-chat-skel-timeline" role="status" aria-label="正在读取消息">
        {MESSAGE_ROWS.map((row, index) => (
            <div className={cn('native-chat-skel', row.mine && 'is-mine')} key={index} aria-hidden>
                <span className="native-chat-skel-avatar" />
                <span className="native-chat-skel-bubble" style={{ width: `${row.width}%` }} />
            </div>))}
    </div>;
}
