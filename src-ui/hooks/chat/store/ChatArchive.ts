import { archiveOf, restoreArchive } from '../../../core/domain/chat/archive';
import { normalizeMessage } from '../../../core/domain/debug/segments';
import { errorText } from '../../../core/domain/errors';
import type { ChatArchivePort } from './chatStore.types';
import type { ChatAccountState } from './ChatAccountState';

export class ChatArchive {
    private restoreRequest: Promise<void> | null = null;
    private saveRequest: Promise<void> | null = null;
    private archiveDirty = false;
    private saveTimer: ReturnType<typeof setTimeout> | undefined;
    constructor(
        private state: ChatAccountState,
        private archive?: ChatArchivePort,
    ) {}
    markDirty() {
        this.archiveDirty = true;
        if (this.state.snapshot.hydrated && !this.state.snapshot.archiveError && !this.saveTimer) {
            this.saveTimer = setTimeout(() => {
                this.saveTimer = undefined;
                void this.flushArchive();
            }, 300);
        }
    }
    restore(): Promise<void> {
        if (this.restoreRequest) return this.restoreRequest;
        if (this.state.snapshot.hydrated || !this.archive) return Promise.resolve();
        this.state.update({ archiveError: '' });
        this.restoreRequest = this.archive
            .load(this.state.session.target.bot_id, this.state.snapshot.account.selfId)
            .then((saved) => {
                const account = saved
                    ? restoreArchive(this.state.snapshot.account, {
                          ...saved,
                          messages: saved.messages.map((message) => ({
                              ...message,
                              segments: this.state.media.externalizeSegments(
                                  normalizeMessage(message.segments),
                              ),
                          })),
                      })
                    : this.state.snapshot.account;
                this.state.account(account);
                this.state.update({ hydrated: true });
                void this.flushArchive();
            })
            .catch((error) =>
                this.state.update({ archiveError: `聊天记录读取失败：${errorText(error)}` }),
            )
            .finally(() => {
                this.restoreRequest = null;
            });
        return this.restoreRequest;
    }
    flushArchive(): Promise<void> {
        clearTimeout(this.saveTimer);
        this.saveTimer = undefined;
        if (this.saveRequest) return this.saveRequest;
        if (!this.archive || !this.state.snapshot.hydrated || !this.archiveDirty)
            return Promise.resolve();
        const archive = this.archive;
        this.saveRequest = Promise.resolve()
            .then(async () => {
                while (this.archiveDirty) {
                    this.archiveDirty = false;
                    try {
                        await archive.save(
                            this.state.session.target.bot_id,
                            this.state.snapshot.account.selfId,
                            archiveOf(this.state.snapshot.account),
                        );
                        this.state.update({ archiveError: '' });
                    } catch (error) {
                        this.archiveDirty = true;
                        this.state.update({
                            archiveError: `聊天记录保存失败：${errorText(error)}`,
                        });
                        break;
                    }
                }
            })
            .finally(() => {
                this.saveRequest = null;
                if (this.archiveDirty && !this.state.snapshot.archiveError)
                    return this.flushArchive();
                return undefined;
            });
        return this.saveRequest;
    }
    async flushForRelease() {
        if (this.restoreRequest) await this.restoreRequest;
        await this.flushArchive();
        if (this.state.snapshot.archiveError) throw new Error(this.state.snapshot.archiveError);
    }
}
