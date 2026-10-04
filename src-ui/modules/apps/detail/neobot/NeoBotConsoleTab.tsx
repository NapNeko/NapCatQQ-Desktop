// NeoBot 的「Web 控制台」页：桌面端只做安装 / 对接 / 启停 / 日志，剩下的功能都在它自己的面板里。
// 这页干两件事——启动控制台，以及把面板**完整能干什么**标出来，省得用户挨个点开找。

import { Button } from '../../../../shared/ui';
import { cn } from '../../../../shared/utils/cn';
import type { AppInstance } from '../../../../core/ipc/types';
import {
    CONSOLE_STATE_LABEL,
    NEOBOT_CONSOLE_FEATURES,
    consoleFeatureStats,
    type ConsoleFeatureState,
} from './neobotConsole';

/** 「桌面端已有」用品牌色、「部分」用警示色、「仅控制台」压暗：一眼看出还差什么 */
function stateTone(state: ConsoleFeatureState): string {
    if (state === 'desktop') return 'text-brand';
    if (state === 'partial') return 'text-warning';
    return 'text-text-tertiary';
}

export const NeoBotConsoleTab: React.FC<{
    instance: AppInstance;
    onOpenWebUi: () => void;
}> = ({ instance, onOpenWebUi }) => {
    const stats = consoleFeatureStats();
    const installing = instance.state === 'installing';

    return (
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
            <section className="rounded-md border border-border-subtle bg-surface px-4 py-3">
                <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                        <h3 className="text-sm font-semibold text-text">NeoBot 自带完整面板</h3>
                        <p className="mt-1 text-xs leading-relaxed text-text-secondary">
                            桌面端管安装、对接 QQ、启停与看日志；模型、提示词、记忆、插件、统计这些在面板里。
                            下面是面板的能力地图，标「仅控制台」的需要到面板操作。
                        </p>
                    </div>
                    <Button
                        variant="primary"
                        size="sm"
                        disabled={installing}
                        onClick={onOpenWebUi}
                    >
                        打开控制台
                    </Button>
                </div>
                {installing && (
                    <p className="mt-2 text-2xs text-text-tertiary">安装完成后才能打开面板。</p>
                )}
                <p className="mt-2 text-2xs leading-snug text-text-tertiary">
                    面板口与 OneBot 口是两个口，别拿实例口去连面板。跨机部署时桌面端会先开隧道，
                    打开的地址是隧道口而不是远端的 127.0.0.1。
                </p>
            </section>

            <p className="text-xs text-text-tertiary">
                面板共 <span className="font-medium text-text-secondary">{stats.total}</span> 项能力：
                桌面端已有 <span className="text-brand">{stats.byState.desktop}</span>、
                部分 <span className="text-warning">{stats.byState.partial}</span>、
                仅控制台 <span className="text-text-secondary">{stats.byState.consoleOnly}</span>
            </p>

            {NEOBOT_CONSOLE_FEATURES.map((group) => (
                <section key={group.id} className="flex flex-col gap-2">
                    <h4 className="text-2xs uppercase tracking-widest text-text-tertiary">
                        {group.title}
                    </h4>
                    <ul className="flex flex-col gap-1.5">
                        {group.items.map((item) => (
                            <li
                                key={item.name}
                                className="rounded-sm border border-border-subtle bg-inset/40 px-3 py-2"
                            >
                                <div className="flex items-baseline justify-between gap-2">
                                    <span className="text-xs font-medium text-text">{item.name}</span>
                                    <span className={cn('shrink-0 text-2xs', stateTone(item.state))}>
                                        {CONSOLE_STATE_LABEL[item.state]}
                                    </span>
                                </div>
                                <p className="mt-0.5 text-2xs leading-snug text-text-secondary">
                                    {item.desc}
                                </p>
                                <p
                                    className="mt-1 truncate font-mono text-2xs text-text-tertiary"
                                    title={item.endpoints.join('   ')}
                                >
                                    {item.endpoints.join('   ')}
                                </p>
                            </li>
                        ))}
                    </ul>
                </section>
            ))}
        </div>
    );
};
