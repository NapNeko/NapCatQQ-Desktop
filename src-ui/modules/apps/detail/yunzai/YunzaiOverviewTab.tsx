// 云崽概览：状态卡回答「云崽现在能不能在 QQ 上响应指令、还差什么」，全卡只给一个主按钮。
// 看三件事：对接、主人、在跑。主人不填也能响应普通指令，但 #更新 #重启 这些管理指令只认主人，
// 群里 #设置主人 还要去日志里抄验证码，桌面端直接填省掉这一步，所以算作一步。

import type { ReactNode } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Blocks, CheckCircle2, Circle, Crown, Link2, Play } from 'lucide-react';
import { Button, Card, Spinner } from '../../../../shared/ui';
import { yunzaiNeedsMaster } from '../../../../core/domain/apps/yunzaiConfig';
import { appFrameworkService } from '../../../../core/services/app-framework.service';
import {
    APP_STORE_GC_MS,
    APP_STORE_STALE_MS,
    appStoreInstalledKey,
} from '../../../../hooks/apps/appStoreQuery';
import { cn } from '../../../../shared/utils/cn';
import type { AppInstance, YunzaiInstanceConfig } from '../../../../core/ipc/types';

type CondKey = 'link' | 'master' | 'run';
type Tone = 'ready' | 'todo' | 'idle';

const TONE_DOT: Record<Tone, string> = {
    ready: 'bg-success ring-success/15',
    todo: 'bg-brand ring-brand/15',
    idle: 'bg-text-disabled ring-text-disabled/15',
};

/** 大多数人装云崽是冲着原神 / 星铁面板来的：没装这两个就提一句 */
const RECOMMENDED: readonly { id: string; label: string }[] = [
    { id: 'genshin', label: '原神基础' },
    { id: 'miao-plugin', label: '喵喵插件' },
];

export const YunzaiOverviewTab: React.FC<{
    instance: AppInstance;
    config: YunzaiInstanceConfig;
    onGoTab: (tab: string) => void;
    onOpenLink: () => void;
    onStart: () => void;
    starting: boolean;
}> = ({ instance, config, onGoTab, onOpenLink, onStart, starting }) => {
    const running = instance.state === 'running';
    const linked = !!instance.link;
    const needsMaster = yunzaiNeedsMaster(config);
    const masterCount = config.other.master_qq.length + config.other.master.length;

    // 和插件页共用一份缓存，切过去不重读
    const installed = useQuery({
        queryKey: appStoreInstalledKey(instance.id, 'plugin'),
        queryFn: () => appFrameworkService.listStoreInstalled(instance.id, 'plugin'),
        staleTime: APP_STORE_STALE_MS,
        gcTime: APP_STORE_GC_MS,
    });
    const missingPlugins = installed.data
        ? RECOMMENDED.filter((r) => !installed.data.some((i) => i.id === r.id))
        : [];

    const conds: { key: CondKey; ok: boolean; label: string }[] = [
        { key: 'link', ok: linked, label: linked ? 'QQ 已对接' : 'QQ 还没对接' },
        {
            key: 'master',
            ok: !needsMaster,
            label: needsMaster ? '还没设主人' : `主人 ${masterCount} 个`,
        },
        { key: 'run', ok: running, label: running ? '运行中' : '已停止' },
    ];
    const missing = conds.filter((c) => !c.ok);
    const next = missing[0]?.key;

    let tone: Tone;
    let title: string;
    let sub: ReactNode;
    let actions: ReactNode = null;
    if (!next) {
        tone = 'ready';
        title = '可以在 QQ 上用云崽了';
        sub = '在群里发 #帮助 看有哪些指令；配置改了几秒内生效，端口、Redis 这些要重启';
    } else if (next === 'run' && missing.length === 1) {
        tone = 'idle';
        title = '都接好了，启动就能用';
        sub = '第一次启动要编译依赖、连 Redis，稍等一会儿；日志里出现「连接成功」就好了';
        actions = <StartButton starting={starting} onStart={onStart} />;
    } else {
        tone = 'todo';
        title = `还差 ${missing.length} 步就能在 QQ 上用`;
        if (next === 'link') {
            sub =
                '先对接同一台机器上的 NapCat / SnowLuma 机器人：Bot 用反向 WS 连到云崽的 /OneBotv11';
            actions = (
                <Button size="sm" variant="primary" onClick={onOpenLink}>
                    <Link2 size={13} />
                    对接
                </Button>
            );
        } else if (next === 'master') {
            sub =
                '#更新、#重启、#设置 这些管理指令只认主人；在这里填 QQ 号，不用去群里发 #设置主人 再抄验证码';
            actions = (
                <Button size="sm" variant="primary" onClick={() => onGoTab('permissions')}>
                    <Crown size={13} />
                    去设主人
                </Button>
            );
        } else {
            sub = '启动后在 QQ 里发 #帮助 试试';
            actions = <StartButton starting={starting} onStart={onStart} />;
        }
    }

    return (
        <div className="flex flex-col gap-4">
            <Card variant="hero" padding="none" className="px-6 py-5">
                <div className="flex flex-wrap items-start justify-between gap-x-6 gap-y-4">
                    <div className="min-w-0 flex-1">
                        <h2 className="flex items-center gap-3 font-display text-[19px] font-semibold leading-snug text-text">
                            <span
                                className={cn(
                                    'h-2 w-2 shrink-0 rounded-full ring-4',
                                    TONE_DOT[tone],
                                )}
                                aria-hidden
                            />
                            {title}
                        </h2>
                        <p className="mt-1.5 pl-5 text-[13px] leading-relaxed text-text-secondary">
                            {sub}
                        </p>
                    </div>
                    {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
                </div>
                <ul className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-2 pl-5">
                    {conds.map((c) => (
                        <li
                            key={c.key}
                            className={cn(
                                'inline-flex items-center gap-1.5 text-xs',
                                c.ok
                                    ? 'text-text-secondary'
                                    : c.key === next
                                      ? 'font-medium text-text'
                                      : 'text-text-tertiary',
                            )}
                        >
                            {c.ok ? (
                                <CheckCircle2 size={14} className="text-success" />
                            ) : (
                                <Circle
                                    size={14}
                                    className={c.key === next ? 'text-brand' : 'text-text-disabled'}
                                />
                            )}
                            {c.label}
                        </li>
                    ))}
                </ul>
            </Card>

            <Card
                padding="none"
                className="flex flex-wrap items-center justify-between gap-3 px-5 py-4"
            >
                <p className="min-w-0 flex-1 text-[13px] leading-relaxed text-text-secondary">
                    {missingPlugins.length > 0
                        ? `云崽本体只带基础指令，原神 / 星铁面板要装${missingPlugins.map((p) => p.label).join('和')}。插件页按插件索引分了功能、游戏、文游、单 JS 几类。`
                        : '插件页按插件索引分了功能、游戏、文游、单 JS 几类；单 JS 插件装完云崽自己热加载，目录插件装完重启一次最稳。'}
                </p>
                <Button size="sm" variant="secondary" onClick={() => onGoTab('plugins')}>
                    <Blocks size={13} />
                    去插件页
                </Button>
            </Card>
        </div>
    );
};

function StartButton({ starting, onStart }: { starting: boolean; onStart: () => void }) {
    return (
        <Button size="sm" variant="primary" disabled={starting} onClick={onStart}>
            {starting ? <Spinner size="sm" className="text-white" /> : <Play size={13} />}
            启动
        </Button>
    );
}
