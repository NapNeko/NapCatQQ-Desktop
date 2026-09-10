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
import { useDesktopConsentGate } from '../../../hooks/desktop/useDesktopConsentGate';
import { requestOnboardingFromSettings } from '../../../hooks/desktop/onboardingHost';
import { useOpenExternal } from '../../../hooks/useOpenExternal';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../../hooks/ui/pushErrorBar';
import { errorText } from '../../../core/domain/errors';
import { DesktopConsentDialog } from '../../../shared/components/next/DesktopConsentDialog';
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
    const consent = useDesktopConsentGate();
    const [openingGuide, setOpeningGuide] = useState(false);

    const [checkState, setCheckState] = useState<CheckState>('idle');
    const [available, setAvailable] = useState<AvailableUpdate | null>(null);

    const handleOpenOnboarding = useCallback(async () => {
        if (openingGuide) return;
        setOpeningGuide(true);
        try {
            // 整条引导：Dialog 认路 → 组件页遮罩（同一流程，无第二入口）
            await requestOnboardingFromSettings();
        } catch (err) {
            pushErrorBar({
                key: 'about-onboarding',
                title: '无法打开入门引导',
                raw: errorText(err),
            });
        } finally {
            setOpeningGuide(false);
        }
    }, [openingGuide]);

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
                });
                return;
            }
            setAvailable(next);
            setCheckState('available');
            pushInfoBar({
                key: 'about-desktop-update',
                tone: 'info',
                title: `发现新版本 ${formatUpdateVersion(next.version)}`,
                content: '可在本页或「组件」页安装。',
            });
        } catch (err) {
            setCheckState('error');
            pushErrorBar({
                key: 'about-desktop-update',
                title: '检查更新失败',
                raw: errorText(err),
            });
        }
    }, []);

    const handleInstallUpdate = useCallback(async () => {
        if (!available || installing) return;
        try {
            pushInfoBar({
                key: 'about-desktop-update',
                tone: 'info',
                title: '正在安装更新',
                content:
                    '应用会短暂退出，安装完成后自动重开；进度见任务队列。',
            });
            await startAction('desktop_self', 'local', 'update');
            // 成功路径后端会 exit；若仍返回则多为 mock / 异常未退出
            pushInfoBar({
                key: 'about-desktop-update',
                tone: 'info',
                title: '安装程序已启动',
                content: '应用未自动退出时手动关闭，等安装跑完。',
            });
        } catch (err) {
            pushErrorBar({
                key: 'about-desktop-update',
                title: '无法开始更新',
                raw: errorText(err),
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
            <SettingsSection title="应用">
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

            <SettingsSection title="开源">
                <FieldRow label="源码仓库" description={APP_GITHUB_REPO}>
                    <Button
                        variant="secondary"
                        size="sm"
                        className="min-w-[80px]"
                        onClick={() => openExternal(APP_GITHUB_URL)}
                    >
                        打开
                        <ExternalLink size={12} strokeWidth={2} className="opacity-70" />
                    </Button>
                </FieldRow>
                <FieldRow label="发行说明">
                    <Button
                        variant="secondary"
                        size="sm"
                        className="min-w-[80px]"
                        onClick={() => openExternal(APP_RELEASES_URL)}
                    >
                        查看
                        <ExternalLink size={12} strokeWidth={2} className="opacity-70" />
                    </Button>
                </FieldRow>
                <FieldRow label="许可" description={APP_LICENSE_SPDX}>
                    <Button
                        variant="secondary"
                        size="sm"
                        className="min-w-[80px]"
                        onClick={() => openExternal(APP_LICENSE_URL)}
                    >
                        查看
                        <ExternalLink size={12} strokeWidth={2} className="opacity-70" />
                    </Button>
                </FieldRow>
                <FieldRow label="用户协议" description="EULA 与隐私说明">
                    <Button
                        variant="secondary"
                        size="sm"
                        className="min-w-[80px]"
                        onClick={() => void consent.openViewer()}
                    >
                        查看
                    </Button>
                </FieldRow>
                <FieldRow
                    label="入门引导"
                    description="重看认路说明与框架对比；不会重走协议门禁"
                    isLast
                >
                    <Button
                        variant="secondary"
                        size="sm"
                        className="min-w-[80px]"
                        disabled={openingGuide}
                        onClick={() => void handleOpenOnboarding()}
                    >
                        {openingGuide ? <Spinner size="xs" tone="brand" /> : null}
                        {openingGuide ? '打开中…' : '重新引导'}
                    </Button>
                </FieldRow>
            </SettingsSection>

            {APP_CREDIT_GROUPS.map((group) => (
                <SettingsSection
                    key={group.title}
                    title={`鸣谢 · ${group.title}`}
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
                                    className="min-w-[80px]"
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

            <DesktopConsentDialog
                open={consent.open}
                mode={consent.mode}
                payload={consent.payload}
                submitting={consent.submitting}
                onAccept={() => void consent.accept()}
                onClose={consent.close}
            />
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


