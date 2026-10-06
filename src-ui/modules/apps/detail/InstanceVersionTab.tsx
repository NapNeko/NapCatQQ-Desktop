// 「版本」页：把版本管理从右上角「更多」里单独拿出来。
//
// 为什么值得单独一页（实测反馈：用户找不到版本入口）：它原先只藏在 ⋯ 菜单里，
// 而「看现在装的哪版、上游最新是哪版、换一版重装」是这类实例最常用的维护动作。
// 摆成一页之后，当前 / 最新 / 可装三件事在一屏里说完，切换也只差一次点击。
//
// 换版本会走完整重装（重新同步依赖、覆盖桌面端写过的脚手架），所以动作仍然交给
// 外壳的 ReinstallDialog 做二次确认，这里只负责「选哪一版」。

import { Badge, Button, Spinner } from '../../../shared/ui';
import { cn } from '../../../shared/utils/cn';
import { hasAppUpdate } from '../../../core/domain/apps/appVersions';
import { isInstalled } from '../../../core/domain/apps/instanceState';
import type { AppInstance } from '../../../core/ipc/types';

export const InstanceVersionTab: React.FC<{
    instance: AppInstance;
    /** 上游最新正式版；null = 查不到或该框架不支持按版本安装 */
    latestVersion: string | null;
    /** 可安装的版本（新到旧）；null = 该框架不支持按版本安装 */
    versions: readonly string[] | null;
    loading: boolean;
    error?: string;
    busy: boolean;
    /** 选定一版去重装；null = 最新正式版 */
    onSwitch: (version: string | null) => void;
}> = ({ instance, latestVersion, versions, loading, error, busy, onSwitch }) => {
    const current = instance.installed_version?.trim() || null;
    const installed = isInstalled(instance);
    const updatable = hasAppUpdate(current, latestVersion);

    return (
        <div className="flex flex-col gap-4">
            <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
                <h3 className="text-sm font-semibold text-text">当前版本</h3>
                <div className="mt-2 flex flex-wrap items-baseline gap-x-4 gap-y-1 text-xs">
                    <span className="text-text-secondary">
                        已安装
                        <span className="ml-1.5 font-mono text-text">
                            {current ? 'v' + current : '未知'}
                        </span>
                    </span>
                    <span className="text-text-secondary">
                        上游最新正式版
                        <span className="ml-1.5 font-mono text-text">
                            {latestVersion ? 'v' + latestVersion : '未知'}
                        </span>
                    </span>
                    {updatable && (
                        <Badge tone="warning" appearance="soft">
                            有新版本
                        </Badge>
                    )}
                </div>
                {!installed && (
                    <p className="mt-2 text-2xs text-text-tertiary">
                        实例还没装好，下面选一版就是安装版本。
                    </p>
                )}
            </section>

            <section className="flex flex-col gap-2">
                <div className="flex items-baseline justify-between gap-2">
                    <h3 className="text-sm font-semibold text-text">可安装的版本</h3>
                    {loading && <Spinner size="sm" />}
                </div>

                {versions === null ? (
                    <p className="text-xs leading-relaxed text-text-secondary">
                        这个框架不支持按版本安装，只能装上游最新正式版—— 在「更多」里点重装即可。
                    </p>
                ) : versions.length === 0 ? (
                    <p className="text-xs leading-relaxed text-text-secondary">
                        {error ? '版本清单没取到：' + error : '没查到可用版本，稍后刷新再试。'}
                    </p>
                ) : (
                    <>
                        <p className="text-xs leading-relaxed text-text-secondary">
                            选一版会走完整重装：重新同步依赖并覆盖桌面端写过的脚手架文件，
                            你自己的插件与配置保持不动。装之前会再确认一次。
                        </p>
                        <ul className="flex flex-col gap-1">
                            {versions.map((v) => {
                                const isCurrent = current === v;
                                const isLatest = latestVersion === v;
                                return (
                                    <li
                                        key={v}
                                        className="flex items-center justify-between gap-2 rounded-sm border border-border-subtle bg-inset/30 px-3 py-1.5"
                                    >
                                        <span className="flex min-w-0 items-baseline gap-2">
                                            <span
                                                className={cn(
                                                    'font-mono text-xs',
                                                    isCurrent ? 'text-brand' : 'text-text',
                                                )}
                                            >
                                                v{v}
                                            </span>
                                            {isCurrent && (
                                                <span className="text-2xs text-brand">当前</span>
                                            )}
                                            {isLatest && (
                                                <span className="text-2xs text-text-tertiary">
                                                    最新正式版
                                                </span>
                                            )}
                                        </span>
                                        <Button
                                            size="sm"
                                            variant={isCurrent ? 'ghost' : 'secondary'}
                                            disabled={busy || (isCurrent && installed)}
                                            onClick={() => onSwitch(v)}
                                        >
                                            {isCurrent && installed
                                                ? '已安装'
                                                : installed
                                                  ? '换到这一版'
                                                  : '装这一版'}
                                        </Button>
                                    </li>
                                );
                            })}
                        </ul>
                    </>
                )}
            </section>
        </div>
    );
};
