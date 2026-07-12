// 关于 Tab：与其它设置 Tab 同一套 SettingsSection / FieldRow 密度。
// 无吉祥物、无 hero 卡；logo 用 Vite ?inline 打成 data URL，随前端资源进包，不走外链路径。

import { useCallback, useState, type ReactNode } from 'react';
import { ExternalLink, RefreshCw } from 'lucide-react';
import {
    APP_GITHUB_REPO,
    APP_GITHUB_URL,
    APP_LICENSE_SPDX,
    APP_LICENSE_URL,
    APP_PRODUCT_NAME,
    APP_RELEASES_URL,
    APP_VERSION_LABEL,
} from '../../../core/domain/app-meta';
import { APP_CREDIT_GROUPS } from '../../../core/domain/credits';
import {
    desktopUpdateService,
    type AvailableUpdate,
} from '../../../core/services/desktop-update.service';
import { useComponentAction } from '../../../hooks/components/useComponentAction';
import { useOpenExternal } from '../../../hooks/useOpenExternal';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { Button, Spinner } from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import { FieldRow, SettingsSection, SettingsTabSections } from '../_shared';
// ?inline → base64 data URL，构建后嵌进 JS bundle，运行时无独立图片路径
import logoAbout from '../../../assets/logo-48.png?inline';

type CheckState = 'idle' | 'checking' | 'latest' | 'available' | 'error';

function formatUpdateVersion(version: string): string {
    const v = version.trim();
    if (!v) return '';
    return v.startsWith('v') || v.startsWith('V') ? v : `v${v}`;
}

export function AboutTab() {
    const openExternal = useOpenExternal();
    const { startAction, isInstalling } = useComponentAction();
    const installing = isInstalling('desktop_self', 'local');

    const [checkState, setCheckState] = useState<CheckState>('idle');
    const [available, setAvailable] = useState<AvailableUpdate | null>(null);

    const handleCheckUpdate = useCallback(async () => {
        setCheckState('checking');
        try {
            const next = await desktopUpdateService.check();
            if (!next) {
                setAvailable(null);
                setCheckState('latest');
                pushInfoBar({
                    key: 'about-desktop-update',
                    tone: 'success',
                    title: '已是最新版本',
                    content: `当前 ${APP_VERSION_LABEL}，无需更新。`,
                });
                return;
            }
            setAvailable(next);
            setCheckState('available');
            pushInfoBar({
                key: 'about-desktop-update',
                tone: 'info',
                title: `发现新版本 ${formatUpdateVersion(next.version)}`,
                content: '可在本页安装，或到「组件」页管理 Desktop 更新。',
            });
        } catch (err) {
            setCheckState('error');
            pushInfoBar({
                key: 'about-desktop-update',
                tone: 'danger',
                title: '检查更新失败',
                content: err instanceof Error ? err.message : String(err),
            });
        }
    }, []);

    const handleInstallUpdate = useCallback(async () => {
        if (!available || installing) return;
        try {
            await startAction('desktop_self', 'local', 'update');
            pushInfoBar({
                key: 'about-desktop-update',
                tone: 'info',
                title: '正在安装更新',
                content: '下载并启动安装程序后应用会退出；进度也可在任务队列查看。',
            });
        } catch (err) {
            pushInfoBar({
                key: 'about-desktop-update',
                tone: 'danger',
                title: '无法开始更新',
                content: err instanceof Error ? err.message : String(err),
            });
        }
    }, [available, installing, startAction]);

    const statusLabel = (() => {
        if (checkState === 'checking') return '检查中…';
        if (checkState === 'latest') return '已是最新';
        if (checkState === 'available' && available) {
            return `可更新至 ${formatUpdateVersion(available.version)}`;
        }
        if (checkState === 'error') return '检查失败';
        return '当前版本';
    })();

    const statusTone =
        checkState === 'latest'
            ? 'success'
            : checkState === 'available'
                ? 'brand'
                : checkState === 'error'
                    ? 'danger'
                    : checkState === 'checking'
                        ? 'muted'
                        : 'neutral';

    return (
        <SettingsTabSections>
            <SettingsSection
                title="应用"
                description="桌面端版本与更新；与组件页 Desktop 更新共用同一套安装链路"
            >
                <div className="flex items-center justify-between gap-6 py-5 first:pt-1 last:pb-1">
                    <div className="flex min-w-0 items-center gap-3">
                        <img
                            src={logoAbout}
                            alt=""
                            width={40}
                            height={40}
                            draggable={false}
                            className="h-10 w-10 shrink-0 rounded-md bg-surface object-contain ring-1 ring-border-subtle"
                        />
                        <div className="min-w-0 space-y-1">
                            <p className="text-[13px] font-medium leading-snug text-text">
                                {APP_PRODUCT_NAME}
                            </p>
                            <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                                <span className="font-mono text-[12px] tabular-nums tracking-tight text-text-secondary">
                                    {APP_VERSION_LABEL}
                                </span>
                                <StatusPill tone={statusTone}>{statusLabel}</StatusPill>
                            </div>
                        </div>
                    </div>

                    <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
                        {checkState === 'available' && available ? (
                            <Button
                                variant="primary"
                                size="sm"
                                disabled={installing}
                                onClick={() => void handleInstallUpdate()}
                            >
                                {installing ? (
                                    <Spinner size="xs" className="text-white" />
                                ) : null}
                                {installing
                                    ? '安装中…'
                                    : `安装 ${formatUpdateVersion(available.version)}`}
                            </Button>
                        ) : null}
                        <Button
                            variant={available ? 'secondary' : 'primary'}
                            size="sm"
                            disabled={checkState === 'checking' || installing}
                            onClick={() => void handleCheckUpdate()}
                        >
                            {checkState === 'checking' ? (
                                <Spinner
                                    size="xs"
                                    tone={available ? 'brand' : 'default'}
                                    className={available ? undefined : 'text-white'}
                                />
                            ) : (
                                <RefreshCw size={14} strokeWidth={2} />
                            )}
                            {checkState === 'checking' ? '检查中…' : '检查更新'}
                        </Button>
                    </div>
                </div>
            </SettingsSection>

            <SettingsSection title="开源" description="源码、发行说明与许可证">
                <FieldRow label="源码仓库" description={APP_GITHUB_REPO}>
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => openExternal(APP_GITHUB_URL)}
                    >
                        在 GitHub 打开
                        <ExternalLink size={12} strokeWidth={2} className="opacity-70" />
                    </Button>
                </FieldRow>
                <FieldRow label="发行说明" description="GitHub Releases">
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => openExternal(APP_RELEASES_URL)}
                    >
                        查看
                        <ExternalLink size={12} strokeWidth={2} className="opacity-70" />
                    </Button>
                </FieldRow>
                <FieldRow label="许可" description={APP_LICENSE_SPDX} isLast>
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => openExternal(APP_LICENSE_URL)}
                    >
                        查看 LICENSE
                        <ExternalLink size={12} strokeWidth={2} className="opacity-70" />
                    </Button>
                </FieldRow>
            </SettingsSection>

            {APP_CREDIT_GROUPS.map((group) => (
                <SettingsSection
                    key={group.title}
                    title={`鸣谢 · ${group.title}`}
                    description={
                        group.title === '前端' ? '感谢这些开源项目' : undefined
                    }
                >
                    {group.items.map((item, index) => (
                        <FieldRow
                            key={item.name}
                            label={item.name}
                            description={`${item.role} · ${item.license}`}
                            isLast={index === group.items.length - 1}
                        >
                            {item.url ? (
                                <Button
                                    variant="secondary"
                                    size="sm"
                                    onClick={() => openExternal(item.url!)}
                                >
                                    主页
                                    <ExternalLink
                                        size={12}
                                        strokeWidth={2}
                                        className="opacity-70"
                                    />
                                </Button>
                            ) : null}
                        </FieldRow>
                    ))}
                </SettingsSection>
            ))}
        </SettingsTabSections>
    );
}

function StatusPill({
    tone,
    children,
}: {
    tone: 'neutral' | 'muted' | 'success' | 'brand' | 'danger';
    children: ReactNode;
}) {
    return (
        <span
            className={cn(
                'inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[11.5px] font-medium',
                tone === 'success' && 'bg-success-soft text-success',
                tone === 'brand' && 'bg-brand-soft text-brand',
                tone === 'danger' && 'bg-danger-soft text-danger',
                tone === 'muted' && 'bg-inset text-text-tertiary',
                tone === 'neutral' && 'text-text-tertiary',
            )}
        >
            {tone !== 'neutral' ? (
                <span
                    className={cn(
                        'h-1.5 w-1.5 rounded-full bg-current',
                        tone === 'muted' && 'animate-pulse',
                    )}
                    aria-hidden
                />
            ) : null}
            {children}
        </span>
    );
}


