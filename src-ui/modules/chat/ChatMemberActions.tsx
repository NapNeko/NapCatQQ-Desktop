// 管理动作逐次确认；上游结果不明确时不重发。
import { useEffect, useRef, useState } from 'react';
import type { Contact } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { ProfileMember } from '../../core/domain/chat/profile';
import { useChatMemberModeration } from '../../hooks/chat/useChatProfile';
import { errorText } from '../../core/domain/errors';
import { Button } from '../../shared/ui/Button';
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogTitle,
} from '../../shared/ui/Dialog';
import { Select } from '../../shared/ui/Select';
import './group-members.css';

export type GroupMemberAction = 'ban' | 'unban' | 'kick';
export type GroupMemberResult = { message: string; tone: 'success' | 'danger' };
export function canManageGroupMember(
    selfId: string,
    selfRole: string | undefined,
    member: ProfileMember,
): boolean {
    if (!member.id || member.id === selfId) return false;
    if (selfRole === 'owner') return member.role === 'admin' || member.role === 'member';
    return selfRole === 'admin' && member.role === 'member';
}
const durations = [
    { value: '600', label: '10 分钟' },
    { value: '3600', label: '1 小时' },
    { value: '86400', label: '1 天' },
    { value: '604800', label: '7 天' },
    { value: '2592000', label: '30 天' },
];

export function ChatMemberActions({
    target,
    contact,
    member,
    action,
    connected,
    onClose,
    onResult,
    onApplied,
}: {
    target: DebugTarget;
    contact: Contact;
    member: ProfileMember;
    action: GroupMemberAction | null;
    connected: boolean;
    onClose: () => void;
    onResult: (result: GroupMemberResult) => void;
    onApplied?: (action: GroupMemberAction) => void;
}) {
    const moderation = useChatMemberModeration(target, contact.id, member.id);
    const [duration, setDuration] = useState('600');
    const busyRef = useRef(false);
    const [busy, setBusy] = useState(false);
    const operation = useRef(0);
    const identity = `${target.bot_id}/${target.qq_id}/${contact.key}/${member.id}`;
    useEffect(() => {
        operation.current++;
        busyRef.current = false;
        setBusy(false);
        setDuration('600');
        return () => {
            operation.current++;
        };
    }, [identity, action]);
    const verb = action === 'kick' ? '移出群' : action === 'unban' ? '解除禁言' : '禁言';
    const submit = async () => {
        if (!action || !connected || busyRef.current || contact.type !== 'group') return;
        const active = action;
        const token = operation.current;
        busyRef.current = true;
        setBusy(true);
        try {
            if (active === 'kick') await moderation.kick.mutateAsync();
            else await moderation.ban.mutateAsync(active === 'unban' ? 0 : Number(duration));
            if (token !== operation.current) return;
            const label = durations.find((value) => value.value === duration)?.label ?? '';
            onResult({
                tone: 'success',
                message:
                    active === 'kick'
                        ? `已将 ${member.name} 移出群`
                        : active === 'unban'
                          ? `已解除 ${member.name} 的禁言`
                          : `已将 ${member.name} 禁言 ${label}`,
            });
            onApplied?.(active);
            onClose();
        } catch (error) {
            if (token !== operation.current) return;
            onResult({ tone: 'danger', message: errorText(error) });
            onClose();
        } finally {
            if (token === operation.current) {
                busyRef.current = false;
                setBusy(false);
            }
        }
    };
    return (
        <Dialog
            open={action !== null}
            onOpenChange={(open) => {
                if (!open && !busyRef.current) onClose();
            }}
        >
            <DialogContent size="sm" className="native-chat-member-action-dialog" hideClose={busy}>
                <DialogTitle>
                    {verb} {member.name}
                </DialogTitle>
                <DialogDescription>
                    {action === 'kick'
                        ? `将 ${member.name}（${member.id}）移出 ${contact.name}，对方仍可申请重新加入。`
                        : `${contact.name} · QQ ${member.id}`}
                </DialogDescription>
                {action === 'ban' && (
                    <Select
                        id="chat-member-ban-duration"
                        label="禁言时长"
                        value={duration}
                        items={durations}
                        disabled={busy}
                        onValueChange={setDuration}
                    />
                )}
                <DialogFooter>
                    <Button disabled={busy} onClick={onClose}>
                        取消
                    </Button>
                    <Button
                        variant={action === 'kick' ? 'danger' : 'primary'}
                        disabled={busy || !connected}
                        onClick={() => void submit()}
                    >
                        {busy ? '正在处理…' : verb}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
