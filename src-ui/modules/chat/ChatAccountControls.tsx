// 聊天账号设置弹窗：账号 / 消息提醒 / 窗口单栏分区，沿用设置页 FormSection 行式。
import { useState, type ReactNode } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { ChevronDown, PanelTop, PanelsTopLeft, RefreshCw, Settings2 } from 'lucide-react';
import { chatDesktopService, isChatPopoutWindow } from '../../core/services/chat-desktop.service';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { ChatAccountPreference } from '../../core/ipc/generated/chat/ChatAccountPreference';
import { Button } from '../../shared/ui/Button';
import { Switch } from '../../shared/ui/Switch';
import { Select } from '../../shared/ui/Select';
import { FormSection } from '../../shared/ui/FormSection';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '../../shared/ui/Dialog';
import { errorText } from '../../core/domain/errors';
import { useChatNotice } from '../../hooks/chat/useChatNotice';
import { useMotion } from '../../hooks/preferences/useMotion';
import type { Contact } from '../../core/domain/chat/model';
import { ChatAvatar } from './ChatAvatar';

function SettingsRow({ id, label, hint, children }: { id?: string; label: string; hint?: string; children: ReactNode }) {
    return (
        <div className="flex items-center justify-between gap-6 py-3.5 first:pt-1 last:pb-1">
            <div className="min-w-0 flex-1 space-y-1">
                <label htmlFor={id} className="block text-[13px] font-medium leading-snug text-text">{label}</label>
                {hint && <p className="text-[12px] leading-relaxed text-text-tertiary">{hint}</p>}
            </div>
            <div className="flex shrink-0 items-center gap-2.5">{children}</div>
        </div>
    );
}

const TRAY_MODES: ReadonlyArray<{ value: ChatAccountPreference['trayNotification']; label: string }> = [
    { value: 'badge', label: '红点' },
    { value: 'flash', label: '头像闪烁' },
    { value: 'off', label: '不提醒' },
];

// 三选段，与设置页 MotionLevelSegment 同款；选中即预览左侧托盘点。
function TrayModeSegment({ value, disabled, onChange }: { value: ChatAccountPreference['trayNotification']; disabled?: boolean; onChange: (value: ChatAccountPreference['trayNotification']) => void }) {
    return (
        <div role="radiogroup" aria-label="托盘提醒" className={'flex h-7 items-center rounded-md bg-inset p-0.5' + (disabled ? ' opacity-50' : '')}>
            {TRAY_MODES.map(item => {
                const selected = value === item.value;
                return (
                    <button
                        key={item.value}
                        type="button"
                        role="radio"
                        aria-checked={selected}
                        disabled={disabled}
                        onClick={() => onChange(item.value)}
                        className={'h-6 rounded-sm px-2.5 text-[12px] font-medium transition-colors ' + (selected
                            ? 'border border-border/50 bg-surface text-text shadow-sm'
                            : 'border border-transparent text-text-tertiary hover:text-text')}
                    >
                        {item.label}
                    </button>
                );
            })}
        </div>
    );
}

export function ChatAccountControls({ target, connectionLabel, onReconnect, contacts = [] }: { target: DebugTarget; connectionLabel: string; onReconnect: () => void | Promise<void>; contacts?: Contact[] }) {
    const [open, setOpen] = useState(false); const [busy, setBusy] = useState(false); const [error, setError] = useState('');
    const motion = useMotion();
    const client = useQueryClient();
    const status = useQuery({ queryKey: ['chat', 'desktop'], queryFn: chatDesktopService.status, enabled: open });
    const accountStatus = status.data?.accounts.find(r => r.target.bot_id === target.bot_id && r.preference.selfId === String(target.qq_id));
    const preference = accountStatus?.preference;
    const noticeKey = `settings:${target.bot_id}:${target.qq_id}`;
    useChatNotice(`${noticeKey}:load`, `${target.name} · 账号设置读取失败`, open && status.isError ? errorText(status.error) : '', () => void status.refetch());
    useChatNotice(`${noticeKey}:account`, `${target.name} · 账号设置异常`, open ? accountStatus?.error : '');
    useChatNotice(`${noticeKey}:action`, `${target.name} · 操作未完成`, error);
    const update = async (patch: Partial<ChatAccountPreference>) => {
        if (!preference) return;
        setBusy(true); setError('');
        try { await chatDesktopService.setPreference({ ...preference, ...patch }); await client.invalidateQueries({ queryKey: ['chat', 'desktop'] }); }
        catch (e) { setError(errorText(e)); }
        finally { setBusy(false); }
    };
    const move = async () => {
        setBusy(true); setError(''); setOpen(false);
        try { if (isChatPopoutWindow()) await chatDesktopService.close(true); else await chatDesktopService.open(target.bot_id); }
        catch (e) { setError(errorText(e)); }
        finally { setBusy(false); }
    };
    const reconnect = async () => {
        setBusy(true); setError('');
        try { await onReconnect(); }
        catch (e) { setError(errorText(e)); }
        finally { setBusy(false); }
    };
    const unmute = async (groupId: string) => {
        setBusy(true); setError('');
        try { await chatDesktopService.ignoreGroup(target.bot_id, String(target.qq_id), groupId, false); await client.invalidateQueries({ queryKey: ['chat', 'desktop'] }); }
        catch (e) { setError(errorText(e)); }
        finally { setBusy(false); }
    };
    const popout = isChatPopoutWindow();
    const enabled = preference?.enabled ?? false;
    const ignored = preference?.ignoredGroups ?? [];
    const avatar = { type: 'private' as const, id: String(target.qq_id), name: target.name };
    return <div className="native-chat-account-controls">
        <Dialog open={open} onOpenChange={setOpen}>
            <DialogTrigger asChild><Button variant="ghost" size="icon" className="native-chat-icon" aria-label="聊天设置" title="聊天设置"><Settings2 size={16} /></Button></DialogTrigger>
            <DialogContent size="md" className="native-chat-settings-dialog" data-motion={motion.enabled ? 'on' : 'off'}>
            <DialogHeader><DialogTitle>聊天设置</DialogTitle></DialogHeader>
            <div className="flex items-center gap-3">
                <ChatAvatar contact={avatar} />
                <div className="min-w-0 flex-1 leading-tight">
                    <div className="truncate text-[13px] font-medium text-text">{target.name}</div>
                    <div className="mt-1 text-[11px] text-text-tertiary">{target.qq_id} · {target.backend === 'snowluma' ? 'SnowLuma' : 'NapCat'}</div>
                </div>
            </div>
            {preference && (
            <div className="scrollbar-hide mt-6 max-h-[min(60vh,560px)] space-y-9 overflow-y-auto">
                <FormSection title="账号" layout="none">
                    <div className="flex flex-col divide-y divide-border-subtle/70">
                        <SettingsRow id="chat-enabled" label="用于聊天" hint={enabled ? '关闭后同时停止后台接收与托盘提醒' : undefined}>
                            <Switch id="chat-enabled" checked={preference.enabled} disabled={busy} onCheckedChange={next => void update(next ? { enabled: next } : { enabled: next, background: false, tray: false })} />
                        </SettingsRow>
                        <SettingsRow id="chat-background" label="后台接收消息">
                            <Switch id="chat-background" checked={preference.background} disabled={busy || !enabled} onCheckedChange={background => void update({ background })} />
                        </SettingsRow>
                        <SettingsRow label="连接">
                            <span className="text-[12px] text-text-tertiary">{connectionLabel}</span>
                            <Button variant="secondary" size="sm" disabled={busy} onClick={() => void reconnect()}><RefreshCw size={13} />{target.running ? '重新连接' : '管理机器人'}</Button>
                        </SettingsRow>
                    </div>
                </FormSection>
                <FormSection title="消息提醒" layout="none">
                    <div className="flex flex-col divide-y divide-border-subtle/70">
                        <SettingsRow id="chat-tray" label="托盘图标">
                            <Switch id="chat-tray" checked={preference.tray} disabled={busy || !enabled} onCheckedChange={tray => void update({ tray })} />
                        </SettingsRow>
                        {preference.tray && (
                        <SettingsRow label="托盘提醒">
                            <span className="native-chat-tray-live" data-flash={preference.trayNotification === 'flash'} aria-hidden>
                                <ChatAvatar contact={avatar} small />
                                {preference.trayNotification !== 'off' && <span className="native-chat-tray-dot" />}
                            </span>
                            <TrayModeSegment value={preference.trayNotification} disabled={busy || !enabled} onChange={trayNotification => void update({ trayNotification })} />
                        </SettingsRow>
                        )}
                        <SettingsRow id="chat-unknown-groups" label="免打扰状态未知的群" hint={target.backend === 'snowluma' ? 'SnowLuma 不提供 QQ 群免打扰状态' : 'QQ 端已免打扰的群不提醒'}>
                            <Select
                                id="chat-unknown-groups"
                                className="w-[128px]"
                                items={[{ value: 'quiet', label: '保持安静' }, { value: 'notify', label: '仍然提醒' }]}
                                value={preference.notifyUnknownGroups ? 'notify' : 'quiet'}
                                disabled={busy || !enabled}
                                onValueChange={value => void update({ notifyUnknownGroups: value === 'notify' })}
                            />
                        </SettingsRow>
                        {ignored.length > 0 && (
                        <details className="native-chat-muted">
                            <summary>
                                <span>本地免打扰群聊</span>
                                <span className="native-chat-muted-count">{ignored.length}</span>
                                <ChevronDown size={14} className="native-chat-muted-chevron" aria-hidden />
                            </summary>
                            <ul className="scrollbar-hide">
                                {ignored.map(groupId => (
                                <li key={groupId}>
                                    <span title={groupId}>{contacts.find(contact => contact.type === 'group' && contact.id === groupId)?.name || groupId}</span>
                                    <Button variant="ghost" size="sm" disabled={busy} onClick={() => void unmute(groupId)}>恢复提醒</Button>
                                </li>
                                ))}
                            </ul>
                        </details>
                        )}
                    </div>
                </FormSection>
                <FormSection title="窗口" layout="none">
                    <div className="flex flex-col divide-y divide-border-subtle/70">
                        <SettingsRow label="聊天窗口">
                            <span className="text-[12px] text-text-tertiary">{popout ? '独立窗口' : '主窗口内'}</span>
                            <Button variant="secondary" size="sm" disabled={busy} onClick={() => void move()}>{popout ? <PanelsTopLeft size={13} /> : <PanelTop size={13} />}{popout ? '嵌回主窗口' : '在独立窗口打开'}</Button>
                        </SettingsRow>
                    </div>
                </FormSection>
            </div>
            )}
            {!preference && status.isLoading && <p role="status" className="mt-6 text-[12px] text-text-tertiary">正在读取账号设置…</p>}
            {!preference && !status.isLoading && (
            <div className="mt-6 flex items-center justify-between gap-4">
                <p className="text-[12px] text-text-tertiary">{status.isError ? errorText(status.error) : '账号设置不可用'}</p>
                <Button variant="ghost" size="sm" onClick={() => void status.refetch()}>重新读取设置</Button>
            </div>
            )}
            </DialogContent>
        </Dialog>
    </div>;
}
