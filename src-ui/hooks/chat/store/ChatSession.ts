import { accountKey, type SessionKey } from '../../../core/domain/chat/model';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import type { ChatTransport } from './chatStore.types';

/** 账号身份与连接代次；只有连接控制器推进代次，所有异步结果校验同一值。 */
export class ChatSession {
    private generation = 0;
    constructor(
        public target: DebugTarget,
        readonly transport: ChatTransport,
    ) {}
    get epoch() {
        return this.generation;
    }
    advance() {
        return ++this.generation;
    }
    peer(key: SessionKey): string | number {
        const value = key.slice(key.indexOf(':') + 1);
        if (this.target.backend === 'snowluma') {
            const numeric = Number(value);
            if (!Number.isSafeInteger(numeric)) throw new Error('账号标识超出可支持范围');
            return numeric;
        }
        return value;
    }
    protocolMessageId(value: string): string | number {
        if (!/^-?\d+$/.test(value) || /^-?0+$/.test(value))
            throw new Error('此消息缺少有效协议标识');
        if (this.target.backend !== 'snowluma') return value;
        const numeric = Number(value);
        if (!Number.isSafeInteger(numeric)) throw new Error('此协议的消息标识超出可支持范围');
        return numeric;
    }
    isCurrent(epoch: number, scope: string): boolean {
        return (
            epoch === this.epoch &&
            scope === accountKey(this.target.bot_id, String(this.target.qq_id))
        );
    }
}
