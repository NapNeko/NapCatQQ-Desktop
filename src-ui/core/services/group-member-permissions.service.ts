// 仅缓存管理入口的显示条件；写操作仍由 runtime 重新验证角色。
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import { chatProfileService, type ProfileMember } from './chat-profile.service';

const TTL = 20_000;
const STALE_TTL = 5 * 60_000;
const FAILED_TTL = 2_000;
const MAX_MEMBERS = 64;
const MAX_QUEUED = 16;
const CONCURRENCY = 2;
export type MemberPermission = {
    status: 'loading' | 'ready' | 'failed';
    member?: ProfileMember;
    expires: number;
    staleUntil?: number;
};
type Job = {
    target: DebugTarget;
    group: string;
    id: string;
    epoch: number;
    promise: Promise<ProfileMember | undefined>;
    resolve: (member: ProfileMember | undefined) => void;
};
const scopeOf = (target: DebugTarget, group: string) =>
    JSON.stringify([target.bot_id, String(target.qq_id), target.backend, group]);

export class GroupMemberPermissions {
    private scope = '';
    private selfId = '';
    private epoch = 0;
    private entries = new Map<string, MemberPermission>();
    private pending = new Map<string, Promise<ProfileMember | undefined>>();
    private queue: Job[] = [];
    private running = 0;
    private listeners = new Set<() => void>();
    constructor(
        private read: typeof chatProfileService.member = (target, group, id) =>
            chatProfileService.member(target, group, id),
        private now = () => Date.now(),
    ) {}
    subscribe = (listener: () => void) => {
        this.listeners.add(listener);
        return () => {
            this.listeners.delete(listener);
        };
    };
    private notify() {
        for (const listener of this.listeners) listener();
    }
    private activate(target: DebugTarget, group: string) {
        const scope = scopeOf(target, group);
        if (scope === this.scope) return;
        this.clear();
        this.scope = scope;
        this.selfId = String(target.qq_id);
    }
    clear(target?: DebugTarget, group?: string) {
        if (target && group && this.scope !== scopeOf(target, group)) return;
        this.epoch++;
        this.scope = '';
        this.entries.clear();
        this.pending.clear();
        for (const job of this.queue.splice(0)) job.resolve(undefined);
        this.notify();
    }
    peek(target: DebugTarget, group: string, id: string): MemberPermission | undefined {
        if (this.scope !== scopeOf(target, group)) return;
        const entry = this.entries.get(id);
        if (!entry) return;
        if (entry.member)
            return (entry.staleUntil ?? entry.expires) > this.now() ? entry : undefined;
        return entry.status === 'loading' || entry.expires > this.now() ? entry : undefined;
    }
    private remember(id: string, entry: MemberPermission): boolean {
        if (!this.entries.has(id) && this.entries.size >= MAX_MEMBERS) {
            const victim = [...this.entries].find(
                ([key, value]) => key !== this.selfId && value.status !== 'loading',
            );
            if (!victim) return false;
            this.entries.delete(victim[0]);
        }
        this.entries.set(id, entry);
        this.notify();
        return true;
    }
    seed(target: DebugTarget, group: string, members: readonly ProfileMember[]) {
        if (this.scope !== scopeOf(target, group)) return;
        for (const member of members.slice(0, MAX_MEMBERS))
            this.remember(member.id, {
                status: 'ready',
                member,
                expires: this.now() + TTL,
                staleUntil: this.now() + STALE_TTL,
            });
    }
    invalidate(target: DebugTarget, group: string, id: string) {
        if (this.scope !== scopeOf(target, group)) return;
        this.entries.delete(id);
        this.notify();
    }
    private load(
        target: DebugTarget,
        group: string,
        id: string,
    ): Promise<ProfileMember | undefined> {
        const cached = this.peek(target, group, id);
        if (cached && cached.status !== 'loading' && cached.expires > this.now())
            return Promise.resolve(cached.member);
        const existing = this.pending.get(id);
        if (existing) return existing;
        if (this.queue.length >= MAX_QUEUED) return Promise.resolve(undefined);
        if (
            !this.remember(id, {
                status: 'loading',
                member: cached?.member,
                expires: cached?.expires ?? Infinity,
                staleUntil: cached?.staleUntil,
            })
        )
            return Promise.resolve(undefined);
        let resolve!: Job['resolve'];
        const promise = new Promise<ProfileMember | undefined>((done) => {
            resolve = done;
        });
        this.pending.set(id, promise);
        this.queue.push({ target: { ...target }, group, id, epoch: this.epoch, promise, resolve });
        this.pump();
        return promise;
    }
    private pump() {
        while (this.running < CONCURRENCY && this.queue.length) {
            const job = this.queue.shift()!;
            this.running++;
            void this.read(job.target, job.group, job.id)
                .then(
                    (member) => {
                        if (job.epoch !== this.epoch) {
                            job.resolve(undefined);
                            return;
                        }
                        this.remember(job.id, {
                            status: 'ready',
                            member,
                            expires: this.now() + TTL,
                            staleUntil: this.now() + STALE_TTL,
                        });
                        job.resolve(member);
                    },
                    () => {
                        if (job.epoch === this.epoch)
                            this.remember(job.id, {
                                status: 'failed',
                                expires: this.now() + FAILED_TTL,
                            });
                        job.resolve(undefined);
                    },
                )
                .finally(() => {
                    if (this.pending.get(job.id) === job.promise) this.pending.delete(job.id);
                    this.running--;
                    this.pump();
                });
        }
    }
    self(target: DebugTarget, group: string) {
        this.activate(target, group);
        return this.load(target, group, String(target.qq_id));
    }
    async warm(
        target: DebugTarget,
        group: string,
        memberId: string,
    ): Promise<ProfileMember | undefined> {
        this.activate(target, group);
        const epoch = this.epoch;
        const self = await this.load(target, group, String(target.qq_id));
        if (
            epoch !== this.epoch ||
            !self ||
            !['owner', 'admin'].includes(self.role) ||
            memberId === self.id
        )
            return;
        return this.load(target, group, memberId);
    }
}
export const groupMemberPermissions = new GroupMemberPermissions();
