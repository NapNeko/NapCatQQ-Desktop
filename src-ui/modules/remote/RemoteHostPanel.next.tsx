// 远端主机管理页（新 UI 树）。
// 功能：列出已保存的 ServerProfile，支持添加 / 测试连接 / 部署 / 删除。

import React, { useState } from 'react';
import { Server, Plus, Wifi, Trash2, Rocket, Loader2 } from 'lucide-react';
import { useServerManager } from '../../hooks/remote/useServerManager';
import { Button } from '../../shared/ui/Button';
import { Card, CardHeader, CardTitle, CardDescription } from '../../shared/ui/Card';
import { Badge } from '../../shared/ui/Badge';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

export const RemoteHostPanelNext: React.FC = () => {
    const {
        servers,
        isLoading,
        addServer,
        isAdding,
        deleteServer,
        testConnection,
        isTesting,
        deploy,
        isDeploying,
    } = useServerManager();

    const [showAdd, setShowAdd] = useState(false);

    return (
        <div className="flex flex-col gap-4 p-4">
            <div className="flex items-center justify-between">
                <h2 className="text-lg font-semibold">远端主机</h2>
                <Button
                    variant="primary"
                    size="sm"
                    onClick={() => setShowAdd(true)}
                >
                    <Plus className="w-4 h-4 mr-1" />
                    添加主机
                </Button>
            </div>

            {isLoading && (
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Loader2 className="w-4 h-4 animate-spin" />
                    加载中...
                </div>
            )}

            {!isLoading && servers.length === 0 && (
                <Card variant="outlined">
                    <CardHeader>
                        <CardTitle className="flex items-center gap-2 text-base">
                            <Server className="w-5 h-5" />
                            暂无远端主机
                        </CardTitle>
                        <CardDescription>
                            点击"添加主机"配置 SSH 连接，即可在远端部署 NapCat。
                        </CardDescription>
                    </CardHeader>
                </Card>
            )}

            <div className="flex flex-col gap-3">
                {servers.map((server) => (
                    <ServerCard
                        key={server.id}
                        server={server}
                        onTest={(pw) => testConnection({ id: server.id, password: pw })}
                        onDelete={() => deleteServer(server.id)}
                        onDeploy={(flavor) => deploy({ id: server.id, flavor })}
                        isTesting={isTesting}
                        isDeploying={isDeploying}
                    />
                ))}
            </div>

            {showAdd && (
                <AddServerDialog
                    onClose={() => setShowAdd(false)}
                    onSubmit={(profile, password) => {
                        addServer({ profile, password });
                        setShowAdd(false);
                    }}
                    isAdding={isAdding}
                />
            )}
        </div>
    );
};

// ─── ServerCard ──────────────────────────────────────────────────────────────

interface ServerCardProps {
    server: ServerProfile;
    onTest: (password?: string) => void;
    onDelete: () => void;
    onDeploy: (flavor: 'NapCat' | 'SnowLuma') => void;
    isTesting: boolean;
    isDeploying: boolean;
}

const ServerCard: React.FC<ServerCardProps> = ({
    server,
    onTest,
    onDelete,
    onDeploy,
    isTesting,
    isDeploying,
}) => {
    const stateColor = {
        disconnected: 'neutral' as const,
        connecting: 'warning' as const,
        connected: 'success' as const,
        failed: 'danger' as const,
    }[server.state];

    const stateLabel = {
        disconnected: '未连接',
        connecting: '连接中',
        connected: '已连接',
        failed: '连接失败',
    }[server.state];

    return (
        <Card variant="ghost" className="p-4">
            <div className="flex items-start justify-between">
                <div className="flex flex-col gap-1">
                    <div className="flex items-center gap-2">
                        <Server className="w-4 h-4 text-muted-foreground" />
                        <span className="font-medium">{server.name}</span>
                        <Badge tone={stateColor} appearance="soft" className="text-xs">
                            {stateLabel}
                        </Badge>
                    </div>
                    <span className="text-sm text-muted-foreground">
                        {server.username}@{server.host}:{server.port}
                    </span>
                </div>
                <div className="flex items-center gap-1">
                    <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => onTest()}
                        disabled={isTesting}
                        title="测试连接"
                    >
                        {isTesting ? (
                            <Loader2 className="w-4 h-4 animate-spin" />
                        ) : (
                            <Wifi className="w-4 h-4" />
                        )}
                    </Button>
                    <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => onDeploy('NapCat')}
                        disabled={isDeploying || server.state !== 'connected'}
                        title="部署 NapCat"
                    >
                        {isDeploying ? (
                            <Loader2 className="w-4 h-4 animate-spin" />
                        ) : (
                            <Rocket className="w-4 h-4" />
                        )}
                    </Button>
                    <Button
                        variant="danger"
                        size="icon"
                        onClick={onDelete}
                        title="删除"
                    >
                        <Trash2 className="w-4 h-4" />
                    </Button>
                </div>
            </div>
        </Card>
    );
};

// ─── AddServerDialog ─────────────────────────────────────────────────────────

interface AddServerDialogProps {
    onClose: () => void;
    onSubmit: (profile: ServerProfile, password?: string) => void;
    isAdding: boolean;
}

const AddServerDialog: React.FC<AddServerDialogProps> = ({ onClose, onSubmit, isAdding }) => {
    const [name, setName] = useState('');
    const [host, setHost] = useState('');
    const [port, setPort] = useState(22);
    const [username, setUsername] = useState('root');
    const [password, setPassword] = useState('');
    const [authMethod, setAuthMethod] = useState<'key' | 'password'>('password');
    const [keyPath, setKeyPath] = useState('');

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        const profile: ServerProfile = {
            id: '',
            name: name || host,
            host,
            port,
            username,
            authMethod,
            privateKeyPath: authMethod === 'key' ? keyPath : null,
            rememberCredential: true,
            state: 'disconnected',
            webuiUrl: null,
        };
        onSubmit(profile, authMethod === 'password' ? password : undefined);
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
            <Card variant="default" className="w-full max-w-md p-6">
                <h3 className="text-base font-semibold mb-4">添加远端主机</h3>
                <form onSubmit={handleSubmit} className="flex flex-col gap-3">
                    <input
                        className="rounded-md border px-3 py-2 text-sm bg-transparent"
                        placeholder="名称（可选）"
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                    />
                    <input
                        className="rounded-md border px-3 py-2 text-sm bg-transparent"
                        placeholder="主机地址 (IP 或域名)"
                        value={host}
                        onChange={(e) => setHost(e.target.value)}
                        required
                    />
                    <div className="flex gap-2">
                        <input
                            className="rounded-md border px-3 py-2 text-sm bg-transparent w-24"
                            type="number"
                            placeholder="端口"
                            value={port}
                            onChange={(e) => setPort(Number(e.target.value))}
                        />
                        <input
                            className="rounded-md border px-3 py-2 text-sm bg-transparent flex-1"
                            placeholder="用户名"
                            value={username}
                            onChange={(e) => setUsername(e.target.value)}
                            required
                        />
                    </div>
                    <select
                        className="rounded-md border px-3 py-2 text-sm bg-transparent"
                        value={authMethod}
                        onChange={(e) => setAuthMethod(e.target.value as 'key' | 'password')}
                    >
                        <option value="password">密码认证</option>
                        <option value="key">密钥认证</option>
                    </select>
                    {authMethod === 'password' && (
                        <input
                            className="rounded-md border px-3 py-2 text-sm bg-transparent"
                            type="password"
                            placeholder="SSH 密码"
                            value={password}
                            onChange={(e) => setPassword(e.target.value)}
                        />
                    )}
                    {authMethod === 'key' && (
                        <input
                            className="rounded-md border px-3 py-2 text-sm bg-transparent"
                            placeholder="私钥文件路径 (~/.ssh/id_ed25519)"
                            value={keyPath}
                            onChange={(e) => setKeyPath(e.target.value)}
                        />
                    )}
                    <div className="flex justify-end gap-2 mt-2">
                        <Button variant="ghost" size="sm" onClick={onClose} type="button">
                            取消
                        </Button>
                        <Button variant="primary" size="sm" type="submit" disabled={isAdding || !host}>
                            {isAdding ? '添加中...' : '添加'}
                        </Button>
                    </div>
                </form>
            </Card>
        </div>
    );
};
