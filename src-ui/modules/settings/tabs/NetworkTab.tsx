// Desktop 代理、下载线路与 GitHub API 设置。

import { useState } from 'react';
import { Check, Info, X } from 'lucide-react';
import {
    Button,
    Popover,
    PopoverContent,
    PopoverTrigger,
    SecretField,
    Select,
    Spinner,
    TextField,
} from '../../../shared/ui';
import { ExpandPresence } from '../../../shared/ui/motion';
import type { DownloadMirror, NetworkSettings } from '../../../core/domain/settings/network';
import { validateNetworkSettings } from '../../../core/domain/settings/network';
import { useNetworkSettings } from '../../../hooks/settings/useNetworkSettings';
import type { SettingsDraft } from '../settings-draft';
import { FieldRow, SettingsSection, SettingsTabSections } from '../_shared';

interface Props {
    draft: SettingsDraft | null;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
    saving?: boolean;
}

const routeItems = [
    { value: 'auto', label: '自动选择' },
    { value: 'direct', label: '仅原站' },
    { value: 'mirror', label: '指定镜像' },
] as const;

function DownloadRoute({
    label,
    mode,
    mirror,
    mirrors,
    error,
    onModeChange,
    onMirrorChange,
}: {
    label: string;
    mode: NetworkSettings['githubDownloadMode'];
    mirror: string;
    mirrors: DownloadMirror[];
    error?: string;
    onModeChange: (mode: NetworkSettings['githubDownloadMode']) => void;
    onMirrorChange: (url: string) => void;
}) {
    const [custom, setCustom] = useState(false);
    const selected = !custom && mirrors.some((item) => item.url === mirror) ? mirror : 'custom';
    return (
        <>
            <FieldRow label={`${label} 下载线路`}>
                <label htmlFor={`network-${label}-route`} className="sr-only">
                    {label} 下载线路
                </label>
                <Select
                    id={`network-${label}-route`}
                    className="w-72 max-w-full"
                    value={mode}
                    onValueChange={onModeChange}
                    items={routeItems}
                />
            </FieldRow>
            <ExpandPresence visible={mode === 'mirror' || !!error}>
                <div className="pt-4">
                    <FieldRow label={`${label} 镜像`}>
                        <div className="flex w-72 min-w-0 max-w-full flex-col gap-2">
                            <label htmlFor={`network-${label}-mirror`} className="sr-only">
                                {label} 镜像
                            </label>
                            <Select
                                id={`network-${label}-mirror`}
                                value={selected}
                                items={[
                                    ...mirrors.map((item) => ({
                                        value: item.url,
                                        label: item.label,
                                    })),
                                    { value: 'custom', label: '自定义地址' },
                                ]}
                                onValueChange={(value) => {
                                    setCustom(value === 'custom');
                                    if (value !== 'custom') onMirrorChange(value);
                                }}
                            />
                            <ExpandPresence visible={selected === 'custom'}>
                                <TextField
                                    aria-label={`${label} 自定义镜像地址`}
                                    value={mirror}
                                    onValueChange={onMirrorChange}
                                    placeholder="https://mirror.example.com"
                                    spellCheck={false}
                                    error={error}
                                    hint={
                                        label === 'Node.js'
                                            ? '填写包含版本目录的 dist 根地址'
                                            : '填写镜像前缀，下载时自动追加完整 GitHub URL'
                                    }
                                />
                            </ExpandPresence>
                        </div>
                    </FieldRow>
                </div>
            </ExpandPresence>
        </>
    );
}

export function NetworkTab({ draft, patchDraft, saving = false }: Props) {
    const probe = useNetworkSettings(draft?.network ?? null, draft?.proxyPassword ?? '');
    if (!draft) return <p className="text-[13px] text-text-tertiary">正在加载设置…</p>;
    const { network } = draft;
    const errors = validateNetworkSettings(network, draft.proxyPassword);
    const patchNetwork = (patch: Partial<NetworkSettings>) =>
        patchDraft({ network: { ...network, ...patch } });

    return (
        <fieldset disabled={saving} className="min-w-0 border-0 p-0">
            {draft.networkError && (
                <p role="alert" className="mb-6 text-[13px] leading-relaxed text-danger">
                    {draft.networkError}
                </p>
            )}
            <SettingsTabSections>
                <SettingsSection
                    title={
                        <span className="inline-flex items-center gap-2">
                            网络代理
                            <Popover>
                                <PopoverTrigger asChild>
                                    <button
                                        type="button"
                                        aria-label="网络设置适用范围"
                                        className="text-text-tertiary hover:text-text"
                                    >
                                        <Info size={14} />
                                    </button>
                                </PopoverTrigger>
                                <PopoverContent className="max-w-80 text-xs leading-relaxed text-text-secondary">
                                    保存后应用于 Desktop 新发起的外网下载与 API
                                    请求，正在进行的请求继续使用原设置。 本机 WebUI、SSH
                                    连接及远端工具自身的网络配置保持原状。
                                </PopoverContent>
                            </Popover>
                        </span>
                    }
                >
                    <FieldRow
                        label="代理模式"
                        description={
                            network.proxyMode === 'system'
                                ? '使用系统静态代理与环境变量，不支持 PAC。'
                                : undefined
                        }
                    >
                        <label htmlFor="network-proxy-mode" className="sr-only">
                            代理模式
                        </label>
                        <Select<NetworkSettings['proxyMode']>
                            id="network-proxy-mode"
                            className="w-72 max-w-full"
                            value={network.proxyMode}
                            onValueChange={(proxyMode) => patchNetwork({ proxyMode })}
                            items={[
                                { value: 'system', label: '跟随系统' },
                                { value: 'direct', label: '不使用代理' },
                                { value: 'manual', label: '手动配置' },
                            ]}
                        />
                    </FieldRow>
                    <ExpandPresence
                        visible={
                            network.proxyMode === 'manual' ||
                            !!errors.proxyUrl ||
                            !!errors.proxyUsername
                        }
                    >
                        <div className="divide-y divide-border-subtle/70 py-4">
                            <FieldRow label="代理地址">
                                <TextField
                                    className="w-72 max-w-full"
                                    aria-label="代理地址"
                                    value={network.proxyUrl}
                                    onValueChange={(proxyUrl) => patchNetwork({ proxyUrl })}
                                    placeholder="http://127.0.0.1:7890"
                                    error={errors.proxyUrl}
                                    hint="HTTP / HTTPS / SOCKS5 / SOCKS5H"
                                    spellCheck={false}
                                />
                            </FieldRow>
                            <FieldRow label="代理用户名（可选）">
                                <TextField
                                    className="w-72 max-w-full"
                                    aria-label="代理用户名"
                                    value={network.proxyUsername}
                                    onValueChange={(proxyUsername) =>
                                        patchNetwork({ proxyUsername })
                                    }
                                    error={errors.proxyUsername}
                                    autoComplete="off"
                                    spellCheck={false}
                                />
                            </FieldRow>
                            <FieldRow label="代理密码（可选）">
                                <SecretField
                                    className="w-72 max-w-full"
                                    aria-label="代理密码"
                                    value={draft.proxyPassword}
                                    onValueChange={(proxyPassword) => patchDraft({ proxyPassword })}
                                    error={errors.proxyPassword}
                                />
                            </FieldRow>
                        </div>
                    </ExpandPresence>
                    <ExpandPresence
                        visible={network.proxyMode !== 'direct' || !!errors.proxyBypass}
                    >
                        <div className="pt-4">
                            <FieldRow label="不使用代理的地址">
                                <TextField
                                    className="w-72 max-w-full"
                                    aria-label="不使用代理的地址"
                                    value={network.proxyBypass}
                                    onValueChange={(proxyBypass) => patchNetwork({ proxyBypass })}
                                    placeholder="localhost,127.0.0.1,::1"
                                    hint="逗号分隔主机、域名、IP 或 CIDR"
                                    error={errors.proxyBypass}
                                    spellCheck={false}
                                />
                            </FieldRow>
                        </div>
                    </ExpandPresence>
                </SettingsSection>

                <SettingsSection
                    title="下载线路"
                    description="自动模式会在失败后尝试其他线路，手动代理下使用原站；指定原站或镜像后，失败时不会自动换源。"
                >
                    <DownloadRoute
                        label="GitHub"
                        mode={network.githubDownloadMode}
                        mirror={network.githubMirror}
                        mirrors={probe.mirrors.filter((item) => item.kind === 'github')}
                        error={errors.githubMirror}
                        onModeChange={(githubDownloadMode) => patchNetwork({ githubDownloadMode })}
                        onMirrorChange={(githubMirror) => patchNetwork({ githubMirror })}
                    />
                    <DownloadRoute
                        label="Node.js"
                        mode={network.nodeDownloadMode}
                        mirror={network.nodeMirror}
                        mirrors={probe.mirrors.filter((item) => item.kind === 'node')}
                        error={errors.nodeMirror}
                        onModeChange={(nodeDownloadMode) => patchNetwork({ nodeDownloadMode })}
                        onMirrorChange={(nodeMirror) => patchNetwork({ nodeMirror })}
                    />
                    {probe.mirrorError && (
                        <p role="alert" className="pt-3 text-xs text-warning">
                            内置线路加载失败，可手动填写镜像地址：{probe.mirrorError}
                        </p>
                    )}
                </SettingsSection>

                <SettingsSection title="GitHub API">
                    <FieldRow label="API 访问方式">
                        <label htmlFor="network-github-api-mode" className="sr-only">
                            API 访问方式
                        </label>
                        <Select<SettingsDraft['githubApiMode']>
                            id="network-github-api-mode"
                            className="w-72 max-w-full"
                            value={draft.githubApiMode}
                            onValueChange={(githubApiMode) => patchDraft({ githubApiMode })}
                            items={[
                                { value: 'proxy', label: '默认中转，失败后尝试原站' },
                                { value: 'direct', label: '仅 GitHub 原站' },
                            ]}
                        />
                    </FieldRow>
                    <FieldRow
                        label="Personal Access Token（可选）"
                        description="仅发送给 GitHub，留空使用匿名额度。保存至系统密钥库或本机加密存储。"
                    >
                        <SecretField
                            className="w-72 max-w-full font-mono"
                            aria-label="GitHub Token"
                            value={draft.githubPat}
                            onValueChange={(githubPat) => patchDraft({ githubPat })}
                            placeholder="ghp_... / github_pat_..."
                        />
                    </FieldRow>
                </SettingsSection>

                <SettingsSection title="连接测试" layout="panel">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <span className="text-xs text-text-tertiary">
                            使用当前代理与下载线路草稿，不会保存设置。
                        </span>
                        <Button
                            variant="secondary"
                            size="sm"
                            onClick={() => void probe.test()}
                            className="shrink-0"
                            disabled={probe.busy || saving || Object.keys(errors).length > 0}
                        >
                            {probe.busy && <Spinner size="xs" />}
                            {probe.busy ? '测试中…' : '测试连接'}
                        </Button>
                    </div>
                    <ExpandPresence visible={!!probe.error}>
                        <p role="alert" className="mt-3 text-xs text-danger">
                            {probe.error}
                        </p>
                    </ExpandPresence>
                    <ExpandPresence visible={probe.results.length > 0}>
                        <ul aria-live="polite" className="mt-4 divide-y divide-border-subtle">
                            {probe.results.map((result, index) => (
                                <li
                                    key={`${result.target}-${index}`}
                                    className="flex items-start gap-3 py-3 text-xs"
                                >
                                    {result.success ? (
                                        <Check
                                            size={15}
                                            className="mt-0.5 shrink-0 text-success"
                                            aria-label="成功"
                                        />
                                    ) : (
                                        <X
                                            size={15}
                                            className="mt-0.5 shrink-0 text-danger"
                                            aria-label="失败"
                                        />
                                    )}
                                    <div className="min-w-0 flex-1 space-y-1">
                                        <p className="font-medium text-text">{result.target}</p>
                                        <p className="break-all text-text-secondary">
                                            {result.message}
                                        </p>
                                        <p className="break-all text-text-tertiary">{result.url}</p>
                                    </div>
                                    <span className="shrink-0 tabular-nums text-text-tertiary">
                                        {Number(result.elapsedMs)} ms
                                    </span>
                                </li>
                            ))}
                        </ul>
                    </ExpandPresence>
                </SettingsSection>
            </SettingsTabSections>
        </fieldset>
    );
}
