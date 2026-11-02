import type { SessionKey } from '../../../core/domain/chat/model';

/** 一条会话的发送锁覆盖整次转发；撤回也参与离开页面后的延迟释放。 */
export class ChatOperations {
    private sends = new Map<SessionKey, number>();
    private recalls = new Map<string, number>();
    constructor(private onIdle: () => void) {}
    isSending(key: SessionKey, epoch: number) {
        return this.sends.get(key) === epoch;
    }
    isRecalling(key: string, epoch: number) {
        return this.recalls.get(key) === epoch;
    }
    startSend(key: SessionKey, epoch: number) {
        this.sends.set(key, epoch);
    }
    startRecall(key: string, epoch: number) {
        this.recalls.set(key, epoch);
    }
    finishSend(key: SessionKey, epoch: number) {
        if (this.sends.get(key) === epoch) this.sends.delete(key);
        if (!this.busy()) this.onIdle();
    }
    finishRecall(key: string, epoch: number) {
        if (this.recalls.get(key) === epoch) this.recalls.delete(key);
        if (!this.busy()) this.onIdle();
    }
    busy() {
        return this.sends.size > 0 || this.recalls.size > 0;
    }
}
