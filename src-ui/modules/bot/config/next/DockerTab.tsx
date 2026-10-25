import { useState } from 'react';
import { Plus, RotateCcw, Trash2 } from 'lucide-react';
import {
    Badge,
    Button,
    FormSection,
    NumberField,
    SecretField,
    Select,
    TextField,
    Switch,
} from '../../../../shared/ui';
import type { BotConfig } from '../../../../core/ipc/generated/domain/BotConfig';
import type { BotDockerConfig } from '../../../../core/ipc/generated/domain/BotDockerConfig';
import {
    defaultBotDockerConfig,
    dockerPortRows,
    setDockerPortMapping,
    validateBotDockerConfig,
    validateDockerEnvironmentKey,
} from '../../../../core/domain/bot/docker-config';
import { ExternalAccessDialog } from './ExternalAccessDialog';

interface DockerTabProps {
    config: BotConfig;
    onChange: (settings: BotDockerConfig) => void;
}

export function DockerTab({ config, onChange }: DockerTabProps) {
    const settings = config.docker ?? defaultBotDockerConfig();
    const rows = dockerPortRows(config);
    const validation = validateBotDockerConfig(config);
    const [extraContainer, setExtraContainer] = useState<number | null>(null);
    const [extraHost, setExtraHost] = useState<number | null>(null);
    const [portError, setPortError] = useState<string | null>(null);
    const [envKey, setEnvKey] = useState('');
    const [envValue, setEnvValue] = useState('');
    const [envError, setEnvError] = useState<string | null>(null);
    const [accessConfirmation, setAccessConfirmation] = useState(false);
    const patch = (next: Partial<BotDockerConfig>) => onChange({ ...settings, ...next });

    const addPort = () => {
        if (!extraContainer || !extraHost) {
            setPortError('请填写容器端口和宿主机端口');
            return;
        }
        if (rows.some((row) => row.container === extraContainer)) {
            setPortError('此容器端口已在上方列出，请直接修改对应映射');
            return;
        }
        const next = setDockerPortMapping(settings, extraContainer, extraHost);
        const error = validateBotDockerConfig({ ...config, docker: next });
        if (error) {
            setPortError(error);
            return;
        }
        onChange(next);
        setExtraContainer(null);
        setExtraHost(null);
        setPortError(null);
    };

    const addEnvironment = () => {
        const error = validateDockerEnvironmentKey(envKey);
        if (error || Object.hasOwn(settings.environment, envKey)) {
            setEnvError(error ?? '此环境变量已存在，请直接修改上方的值');
            return;
        }
        patch({ environment: { ...settings.environment, [envKey]: envValue } });
        setEnvKey('');
        setEnvValue('');
        setEnvError(null);
    };

    return (
        <div className="flex min-w-0 flex-col gap-7">
            <p className="rounded-md border border-border-subtle bg-field/35 px-4 py-3 text-xs leading-relaxed text-text-secondary">
                保存后自动更新容器配置。运行中的 Bot
                在映射或容器设置变化时会重建容器，连接会短暂中断；数据卷保留。 停止的 Bot
                在下次启动时应用。自定义设置会保存在此 Bot 中。
            </p>
            {validation && (
                <p role="alert" className="text-xs text-danger">
                    {validation}
                </p>
            )}
            <FormSection title="访问范围">
                <Switch
                    label="允许主机外部访问"
                    hint="默认仅发布到 127.0.0.1，可通过 SSH 隧道使用。开启后所有映射发布到 0.0.0.0。"
                    checked={settings.publishOnAllInterfaces ?? false}
                    onCheckedChange={(open) =>
                        open
                            ? setAccessConfirmation(true)
                            : patch({ publishOnAllInterfaces: false })
                    }
                />
                <p className="text-xs text-text-secondary">
                    配置发布地址：
                    <span className="font-mono">
                        {settings.publishOnAllInterfaces ? '0.0.0.0' : '127.0.0.1'}
                    </span>
                    {settings.publishOnAllInterfaces && (
                        <span className="ml-2 text-warning">
                            主机外部可访问，请配置鉴权和防火墙
                        </span>
                    )}
                </p>
            </FormSection>
            <FormSection
                title="端口映射"
                description="启用的 HTTP、SSE 和 WebSocket 监听会自动发布 TCP 端口。宿主机端口默认加上账号偏移，可在此调整以避免冲突。"
            >
                <div className="overflow-hidden rounded-md border border-border-subtle">
                    <div className="grid grid-cols-[minmax(0,1fr)_minmax(5rem,8rem)_2rem] items-center gap-3 bg-field/50 px-3 py-2 text-2xs text-text-tertiary sm:grid-cols-[minmax(0,1fr)_minmax(6rem,10rem)_5.5rem]">
                        <span>容器端口 / 用途</span>
                        <span>宿主机端口</span>
                        <span className="sr-only">操作</span>
                    </div>
                    <div className="divide-y divide-border-subtle">
                        {rows.map((row) => (
                            <div
                                key={row.container}
                                className="grid grid-cols-[minmax(0,1fr)_minmax(5rem,8rem)_2rem] items-center gap-3 px-3 py-3 sm:grid-cols-[minmax(0,1fr)_minmax(6rem,10rem)_5.5rem]"
                            >
                                <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                                    <span className="font-mono text-sm tabular-nums text-text">
                                        {row.container}
                                    </span>
                                    <span className="min-w-0 truncate text-xs text-text-secondary">
                                        {row.label}
                                    </span>
                                    <Badge>{row.automatic ? '自动' : '自定义'}</Badge>
                                </div>
                                <NumberField
                                    aria-label={`容器端口 ${row.container} 的宿主机端口`}
                                    value={row.host}
                                    min={1}
                                    max={65535}
                                    onValueChange={(value) =>
                                        onChange(
                                            setDockerPortMapping(
                                                settings,
                                                row.container,
                                                value ?? 0,
                                            ),
                                        )
                                    }
                                />
                                {!row.automatic && (
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        aria-label={
                                            row.required
                                                ? `恢复端口 ${row.container} 的自动映射`
                                                : `删除端口 ${row.container} 的映射`
                                        }
                                        title={row.required ? '恢复自动' : '删除映射'}
                                        onClick={() =>
                                            patch({
                                                portMappings: settings.portMappings.filter(
                                                    (mapping) =>
                                                        mapping.container !== row.container,
                                                ),
                                            })
                                        }
                                    >
                                        {row.required ? (
                                            <RotateCcw size={14} />
                                        ) : (
                                            <Trash2 size={14} />
                                        )}
                                    </Button>
                                )}
                            </div>
                        ))}
                    </div>
                </div>
                <div className="grid grid-cols-1 items-end gap-3 rounded-md bg-field/30 p-3 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
                    <NumberField
                        label="额外容器端口"
                        value={extraContainer}
                        onValueChange={setExtraContainer}
                        min={1}
                        max={65535}
                    />
                    <NumberField
                        label="额外宿主机端口"
                        value={extraHost}
                        onValueChange={setExtraHost}
                        min={1}
                        max={65535}
                    />
                    <Button variant="secondary" size="sm" onClick={addPort}>
                        <Plus size={13} />
                        添加映射
                    </Button>
                </div>
                {portError && (
                    <p role="alert" className="text-xs text-danger">
                        {portError}
                    </p>
                )}
                <p className="text-2xs leading-relaxed text-text-tertiary">
                    默认端口映射沿用现有部署。新增监听仅在启用时自动添加；反向 WebSocket 和 HTTP
                    客户端无需发布端口。 宿主机防火墙需另行放行。
                </p>
            </FormSection>

            <FormSection title="运行策略" layout="grid-2">
                <Select
                    label="Docker 重启策略"
                    value={settings.restartPolicy}
                    onValueChange={(restartPolicy) => patch({ restartPolicy })}
                    items={[
                        { value: 'unless-stopped', label: '除非手动停止（默认）' },
                        { value: 'always', label: '始终重启' },
                        { value: 'on-failure', label: '仅异常退出时重启' },
                        { value: 'no', label: '不自动重启' },
                    ]}
                />
                <NumberField
                    label="内存上限（MiB）"
                    value={settings.memoryMb}
                    min={1}
                    placeholder="不限"
                    hint="留空表示不限；上限过低可能导致 QQ 被终止。"
                    onValueChange={(memoryMb) => patch({ memoryMb: memoryMb ?? undefined })}
                />
            </FormSection>

            <FormSection
                title="额外环境变量"
                description="仅填写容器需要的附加变量。账号、凭据、用户和指标探针变量由 Desktop 管理。"
            >
                {Object.entries(settings.environment).length === 0 && (
                    <p className="text-xs text-text-tertiary">尚未添加额外环境变量</p>
                )}
                {Object.entries(settings.environment).map(([key, value]) => (
                    <div key={key} className="flex items-end gap-3">
                        <SecretField
                            label={key || '（空变量名）'}
                            value={value}
                            onValueChange={(nextValue) =>
                                patch({
                                    environment: { ...settings.environment, [key]: nextValue },
                                })
                            }
                            className="min-w-0 flex-1 font-mono"
                        />
                        <Button
                            variant="ghost"
                            size="icon"
                            aria-label={`删除环境变量 ${key}`}
                            onClick={() => {
                                const next = { ...settings.environment };
                                delete next[key];
                                patch({ environment: next });
                            }}
                        >
                            <Trash2 size={14} />
                        </Button>
                    </div>
                ))}
                <div className="grid grid-cols-1 items-end gap-3 rounded-md bg-field/30 p-3 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
                    <TextField
                        label="新变量名"
                        value={envKey}
                        onValueChange={setEnvKey}
                        placeholder="TZ"
                        className="font-mono"
                    />
                    <SecretField
                        label="新变量值"
                        value={envValue}
                        onValueChange={setEnvValue}
                        placeholder="Asia/Shanghai"
                        className="font-mono"
                    />
                    <Button variant="secondary" size="sm" onClick={addEnvironment}>
                        <Plus size={13} />
                        添加变量
                    </Button>
                </div>
                {envError && (
                    <p role="alert" className="text-xs text-danger">
                        {envError}
                    </p>
                )}
            </FormSection>
            <ExternalAccessDialog
                open={accessConfirmation}
                docker
                onOpenChange={setAccessConfirmation}
                onConfirm={() => {
                    patch({ publishOnAllInterfaces: true });
                    setAccessConfirmation(false);
                }}
            />
        </div>
    );
}
