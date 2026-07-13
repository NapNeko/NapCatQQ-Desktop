// 邮件通知连接配置 Dialog

import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    NumberField,
    Select,
    TextField,
} from '../../../../shared/ui';
import type { SettingsDraft } from '../../settings-draft';
import { DialogField, SecretField } from './dialog-shared';

const ENCRYPTION_ITEMS = [
    { value: 'SSL', label: 'SSL' },
    { value: 'TLS', label: 'TLS' },
    { value: '无加密', label: '无加密' },
] as const;

const EMAIL_PRESETS = [
    { label: 'QQ 邮箱', server: 'smtp.qq.com', port: 465, encryption: 'SSL' },
    { label: '163 邮箱', server: 'smtp.163.com', port: 465, encryption: 'SSL' },
    { label: 'Gmail', server: 'smtp.gmail.com', port: 465, encryption: 'SSL' },
    {
        label: 'Outlook',
        server: 'smtp.office365.com',
        port: 587,
        encryption: 'TLS',
    },
] as const;

const EMAIL_PRESET_ITEMS = EMAIL_PRESETS.map((preset) => ({
    value: preset.label,
    label: preset.label,
}));

export type EmailEditorDraft = Pick<
    SettingsDraft,
    | 'emailSender'
    | 'emailReceiver'
    | 'emailToken'
    | 'emailSmtpServer'
    | 'emailSmtpPort'
    | 'emailEncryption'
>;

export function emailIsReady(email: EmailEditorDraft): boolean {
    return Boolean(
        email.emailSender.trim() &&
            email.emailReceiver.trim() &&
            email.emailToken.trim() &&
            email.emailSmtpServer.trim() &&
            email.emailSmtpPort > 0,
    );
}

export function emailSummary(email: EmailEditorDraft): string {
    if (!email.emailSmtpServer.trim()) return '尚未选择邮箱服务';
    if (!email.emailSender.trim() || !email.emailReceiver.trim()) {
        return `${email.emailSmtpServer} · 待填写收发地址`;
    }
    if (!email.emailToken.trim()) return `${email.emailSmtpServer} · 缺授权码`;
    return `${email.emailSender.trim()} → ${email.emailReceiver.trim()}`;
}

export function EmailEditorDialog({
    open,
    draft,
    preset,
    onOpenChange,
    onPresetChange,
    onDraftChange,
    onSave,
}: {
    open: boolean;
    draft: EmailEditorDraft;
    preset: string;
    onOpenChange: (open: boolean) => void;
    onPresetChange: (preset: string) => void;
    onDraftChange: (patch: Partial<EmailEditorDraft>) => void;
    onSave: () => void;
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent size="md" dismissOnOutsideClick={false}>
                <DialogHeader>
                    <DialogTitle>配置邮件通知</DialogTitle>
                    <DialogDescription>
                        选择常用邮箱后补齐收发地址和授权码。完成后回到设置页保存。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-5 py-1">
                    <DialogField
                        label="常用邮箱"
                        hint="选择后会自动填入服务器、端口和加密方式。"
                    >
                        <Select
                            value={preset}
                            placeholder="选择服务商，或手动填写"
                            onValueChange={(label) => {
                                onPresetChange(label);
                                const found = EMAIL_PRESETS.find(
                                    (item) => item.label === label,
                                );
                                if (!found) return;
                                onDraftChange({
                                    emailSmtpServer: found.server,
                                    emailSmtpPort: found.port,
                                    emailEncryption: found.encryption,
                                });
                            }}
                            items={EMAIL_PRESET_ITEMS}
                        />
                    </DialogField>

                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                        <DialogField label="发件邮箱">
                            <TextField
                                name="email-sender"
                                type="email"
                                autoComplete="off"
                                spellCheck={false}
                                value={draft.emailSender}
                                placeholder="you@example.com"
                                onValueChange={(emailSender) =>
                                    onDraftChange({ emailSender })
                                }
                            />
                        </DialogField>
                        <DialogField label="收件邮箱">
                            <TextField
                                name="email-receiver"
                                type="email"
                                autoComplete="off"
                                spellCheck={false}
                                value={draft.emailReceiver}
                                placeholder="alert@example.com"
                                onValueChange={(emailReceiver) =>
                                    onDraftChange({ emailReceiver })
                                }
                            />
                        </DialogField>
                    </div>

                    <DialogField
                        label="授权码"
                        hint="使用邮箱服务商生成的 SMTP 授权码，不是登录密码。"
                    >
                        <SecretField
                            name="email-token"
                            value={draft.emailToken}
                            placeholder="输入授权码"
                            onValueChange={(emailToken) =>
                                onDraftChange({ emailToken })
                            }
                        />
                    </DialogField>

                    <div className="grid grid-cols-[minmax(0,1fr)_6.5rem] gap-3">
                        <DialogField label="SMTP 服务器">
                            <TextField
                                name="email-smtp"
                                autoComplete="off"
                                spellCheck={false}
                                value={draft.emailSmtpServer}
                                placeholder="smtp.example.com"
                                onValueChange={(emailSmtpServer) =>
                                    onDraftChange({ emailSmtpServer })
                                }
                            />
                        </DialogField>
                        <DialogField label="端口">
                            <NumberField
                                name="email-port"
                                value={draft.emailSmtpPort}
                                min={1}
                                max={65535}
                                onValueChange={(value) =>
                                    onDraftChange({
                                        emailSmtpPort: Math.max(
                                            1,
                                            Math.min(
                                                65535,
                                                Math.round(value || 1),
                                            ),
                                        ),
                                    })
                                }
                            />
                        </DialogField>
                    </div>

                    <DialogField label="连接加密">
                        <Select
                            value={draft.emailEncryption || 'SSL'}
                            onValueChange={(emailEncryption) =>
                                onDraftChange({ emailEncryption })
                            }
                            items={[...ENCRYPTION_ITEMS]}
                        />
                    </DialogField>
                </div>
                <DialogFooter>
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button type="button" size="sm" onClick={onSave}>
                        完成
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
