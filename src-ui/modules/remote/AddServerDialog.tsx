// 服务器档案表单弹窗：新增 / 编辑两用。
//
// 走 shared/ui Dialog（Radix），输入框走"裸 input + bg-inset/transparent"
// 风格，对齐 BotLogPage.next 的搜索框语言。
//
// initialProfile 为 null = 新增模式；非空 = 编辑模式（带 id，提交走 update）。
// 编辑时表单预填该档案字段；密码框留空表示"不改已存的凭据"。
//
// 提交时：远端页只管"档案"——password 仅作可选凭据存进 keyring，是否记忆
// 由 rememberCredential 字段控制；连接测试由用户在卡片上点"测试连接"完成。

import React, { useEffect, useState } from 'react';
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
    DialogClose,
    Button,
} from '../../shared/ui';
import { serverService } from '../../core/services/server.service';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import type { AuthMethod } from '../../core/ipc/generated/domain/AuthMethod';

interface AddServerDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    isSubmitting: boolean;
    /// null = 新增；非空 = 编辑该档案。
    initialProfile?: ServerProfile | null;
    onSubmit: (profile: ServerProfile, password: string | undefined, autoKey: boolean) => void;
}

export const AddServerDialog: React.FC<AddServerDialogProps> = ({
    open,
    onOpenChange,
    isSubmitting,
    initialProfile,
    onSubmit,
}) => {
    const isEdit = !!initialProfile;

    const [name, setName] = useState('');
    const [host, setHost] = useState('');
    const [port, setPort] = useState(22);
    const [username, setUsername] = useState('root');
    const [password, setPassword] = useState('');
    const [authMethod, setAuthMethod] = useState<AuthMethod>('password');
    const [keyPath, setKeyPath] = useState('');
    const [remember, setRemember] = useState(true);
    // 密码模式下勾选：添加后用这次密码自动配置免密登录（推荐）。
    const [autoKey, setAutoKey] = useState(true);

    // 打开弹窗时按模式初始化表单：编辑预填档案字段，新增回到默认值。
    // 密码框永远从空开始——编辑时留空表示不动已存凭据。
    useEffect(() => {
        if (!open) return;
        if (initialProfile) {
            setName(initialProfile.name);
            setHost(initialProfile.host);
            setPort(initialProfile.port);
            setUsername(initialProfile.username);
            setAuthMethod(initialProfile.authMethod);
            setKeyPath(initialProfile.privateKeyPath ?? '');
            setRemember(initialProfile.rememberCredential);
        } else {
            setName('');
            setHost('');
            setPort(22);
            setUsername('root');
            setAuthMethod('password');
            setKeyPath('');
            setRemember(true);
        }
        setPassword('');
    }, [open, initialProfile]);

    // 切到密钥认证 / 打开弹窗时扫一次本地 ~/.ssh/。
    // 候选项不为空时默认选第一个（ed25519 优先），用户也可手填路径。
    const [scannedKeys, setScannedKeys] = useState<string[]>([]);
    useEffect(() => {
        if (!open || authMethod !== 'key') return;
        let cancelled = false;
        serverService.scanLocalSshKeys().then((keys) => {
            if (cancelled) return;
            setScannedKeys(keys);
            // keyPath 为空时自动填第一个候选；用户已选则不覆盖。
            if (keys.length > 0) {
                setKeyPath((current) => current || keys[0]);
            }
        });
        return () => {
            cancelled = true;
        };
    }, [open, authMethod]);

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        const profile: ServerProfile = {
            id: initialProfile?.id ?? '',
            name: name.trim() || host.trim(),
            host: host.trim(),
            port,
            username: username.trim(),
            authMethod,
            privateKeyPath: authMethod === 'key' ? keyPath.trim() || null : null,
            rememberCredential: remember,
            // 编辑时保留原状态（连接态由 test_connection 维护，表单不该重置它）。
            state: initialProfile?.state ?? 'disconnected',
            webuiUrl: initialProfile?.webuiUrl ?? null,
        };
        // 密码为空：新增=不存凭据；编辑=不改已存凭据。两种都传 undefined。
        const pw = authMethod === 'password' && password ? password : undefined;
        // 仅新增 + 密码模式 + 填了密码 + 勾选时，才在添加后配免密。
        const wantAutoKey = !isEdit && authMethod === 'password' && !!password && autoKey;
        onSubmit(profile, pw, wantAutoKey);
    };

    const canSubmit = host.trim().length > 0 && username.trim().length > 0;

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="max-w-md">
                <DialogHeader>
                    <DialogTitle>{isEdit ? '编辑远端服务器' : '添加远端服务器'}</DialogTitle>
                    <DialogDescription>
                        {isEdit
                            ? '修改 SSH 连接信息。密码留空表示不改动已保存的凭据。'
                            : '配置 SSH 连接信息。凭据将通过系统 keyring 加密存储。'}
                    </DialogDescription>
                </DialogHeader>

                <form onSubmit={handleSubmit} className="flex flex-col gap-3">
                    <Field label="名称">
                        <TextInput
                            placeholder="例如：生产服务器（留空则使用主机名）"
                            value={name}
                            onChange={setName}
                        />
                    </Field>

                    <Field label="主机地址" required>
                        <TextInput
                            placeholder="IP 或域名"
                            value={host}
                            onChange={setHost}
                            autoFocus
                        />
                    </Field>

                    <div className="grid grid-cols-[80px_1fr] gap-2">
                        <Field label="端口">
                            <TextInput
                                type="number"
                                value={String(port)}
                                onChange={(v) => setPort(Number(v) || 22)}
                            />
                        </Field>
                        <Field label="用户名" required>
                            <TextInput
                                placeholder="ubuntu / root"
                                value={username}
                                onChange={setUsername}
                            />
                        </Field>
                    </div>

                    <Field label="认证方式">
                        <select
                            className="h-8 w-full rounded-sm bg-inset px-2 text-sm text-text outline-none focus:ring-1 focus:ring-brand"
                            value={authMethod}
                            onChange={(e) => setAuthMethod(e.target.value as AuthMethod)}
                        >
                            <option value="password">密码认证</option>
                            <option value="key">密钥认证</option>
                        </select>
                    </Field>

                    {authMethod === 'password' && (
                        <Field label="SSH 密码">
                            <TextInput
                                type="password"
                                placeholder={
                                    isEdit
                                        ? '留空表示不改动已保存的凭据'
                                        : '留空时仅添加档案，下次连接时再填'
                                }
                                value={password}
                                onChange={setPassword}
                            />
                        </Field>
                    )}

                    {!isEdit && authMethod === 'password' && (
                        <label className="flex cursor-pointer items-center gap-2 text-xs text-text-secondary">
                            <input
                                type="checkbox"
                                className="h-3.5 w-3.5 rounded-sm accent-brand"
                                checked={autoKey}
                                onChange={(e) => setAutoKey(e.target.checked)}
                            />
                            <span>自动配置免密登录（推荐）</span>
                        </label>
                    )}

                    {authMethod === 'key' && (
                        <Field label="私钥文件路径">
                            {scannedKeys.length > 0 ? (
                                <div className="flex flex-col gap-1.5">
                                    <select
                                        className="h-8 w-full rounded-sm bg-inset px-2 text-sm text-text outline-none focus:ring-1 focus:ring-brand"
                                        value={
                                            scannedKeys.includes(keyPath) ? keyPath : '__custom__'
                                        }
                                        onChange={(e) => {
                                            if (e.target.value === '__custom__') {
                                                setKeyPath('');
                                            } else {
                                                setKeyPath(e.target.value);
                                            }
                                        }}
                                    >
                                        {scannedKeys.map((path) => (
                                            <option key={path} value={path}>
                                                {fileName(path)}
                                            </option>
                                        ))}
                                        <option value="__custom__">自定义路径…</option>
                                    </select>
                                    {!scannedKeys.includes(keyPath) && (
                                        <TextInput
                                            placeholder="例：~/.ssh/id_ed25519"
                                            value={keyPath}
                                            onChange={setKeyPath}
                                        />
                                    )}
                                    <p className="text-2xs text-text-tertiary">
                                        已在 ~/.ssh/ 中发现 {scannedKeys.length} 个标准密钥
                                    </p>
                                </div>
                            ) : (
                                <div className="flex flex-col gap-1">
                                    <TextInput
                                        placeholder="例：~/.ssh/id_ed25519"
                                        value={keyPath}
                                        onChange={setKeyPath}
                                    />
                                    <p className="text-2xs text-text-tertiary">
                                        ~/.ssh/ 下未发现标准命名密钥，请手动填路径
                                    </p>
                                </div>
                            )}
                        </Field>
                    )}

                    <label className="mt-1 flex cursor-pointer items-center gap-2 text-xs text-text-secondary">
                        <input
                            type="checkbox"
                            className="h-3.5 w-3.5 rounded-sm accent-brand"
                            checked={remember}
                            onChange={(e) => setRemember(e.target.checked)}
                        />
                        <span>记住凭据（存入系统 keyring）</span>
                    </label>

                    <DialogFooter>
                        <DialogClose asChild>
                            <Button size="sm" variant="ghost" type="button">
                                取消
                            </Button>
                        </DialogClose>
                        <Button
                            size="sm"
                            variant="primary"
                            type="submit"
                            disabled={!canSubmit || isSubmitting}
                        >
                            {isSubmitting ? (isEdit ? '保存中…' : '添加中…') : isEdit ? '保存' : '添加'}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    );
};

// ─── Field / TextInput 子件 ──────────────────────────────────────────────

interface FieldProps {
    label: string;
    required?: boolean;
    children: React.ReactNode;
}

const Field: React.FC<FieldProps> = ({ label, required, children }) => (
    <div className="flex flex-col gap-1">
        <span className="text-2xs font-medium text-text-secondary">
            {label}
            {required && <span className="ml-0.5 text-danger">*</span>}
        </span>
        {children}
    </div>
);

interface TextInputProps {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
    type?: 'text' | 'number' | 'password';
    autoFocus?: boolean;
}

const TextInput: React.FC<TextInputProps> = ({
    value,
    onChange,
    placeholder,
    type = 'text',
    autoFocus,
}) => (
    <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        autoFocus={autoFocus}
        className="h-8 w-full rounded-sm bg-inset px-2 text-sm text-text outline-none transition-colors placeholder:text-text-tertiary focus:ring-1 focus:ring-brand"
    />
);

/// 取路径最后一段（兼容 / 和 \）。
function fileName(path: string): string {
    const idx = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    return idx >= 0 ? path.slice(idx + 1) : path;
}
