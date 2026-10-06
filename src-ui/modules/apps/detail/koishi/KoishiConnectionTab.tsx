// 连接：和协议 Bot 的对接状态（由对接对话框写，这里只读）。端口在「服务器」页改。

import { CheckCircle2, Circle, Link2, Puzzle } from 'lucide-react';
import { Button, FormSection } from '../../../../shared/ui';
import { CopyCodeBlock } from '../../../../shared/ui/CopyCodeBlock';
import { cn } from '../../../../shared/utils/cn';
import { ConfigForm } from '../karin/configLayout';
import {
    KOISHI_REVERSE_WS_PATH,
    effective,
    isLinkNode,
    koishiServer,
    linkNode,
} from '../../../../core/domain/apps/koishiConfig';
import type { AppInstance, KoishiInstanceConfig } from '../../../../core/ipc/types';

function Row({ ok, title, children }: { ok: boolean; title: string; children: React.ReactNode }) {
    return (
        <li className="flex items-start gap-3 py-3 first:pt-0 last:pb-0">
            {ok ? (
                <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-success" />
            ) : (
                <Circle size={16} className="mt-0.5 shrink-0 text-text-disabled" />
            )}
            <div className="min-w-0 flex-1">
                <p className="text-[13px] font-medium text-text">{title}</p>
                <div className="mt-0.5 text-xs leading-relaxed text-text-tertiary">{children}</div>
            </div>
        </li>
    );
}

export const KoishiConnectionTab: React.FC<{
    instance: AppInstance;
    config: KoishiInstanceConfig;
    onGoTab: (tab: string) => void;
    onOpenLink: () => void;
}> = ({ instance, config, onGoTab, onOpenLink }) => {
    const server = koishiServer(config);
    const entry = linkNode(config);
    const entryOn = effective(config.plugins).some(isLinkNode);
    const linked = !!instance.link;
    const selfId = typeof entry?.config.selfId === 'string' ? entry.config.selfId : '';
    const url = `ws://127.0.0.1:${server.port}${KOISHI_REVERSE_WS_PATH}`;

    return (
        <ConfigForm>
            <FormSection
                title="对接协议 Bot"
                description="Bot 作 WebSocket 客户端反向连到 Koishi；Koishi 按请求头里的 QQ 号认 Bot"
                actions={
                    <Button
                        size="sm"
                        variant={linked ? 'secondary' : 'primary'}
                        onClick={onOpenLink}
                    >
                        <Link2 size={13} />
                        {linked ? '换一个 Bot' : '对接'}
                    </Button>
                }
            >
                <ul className="flex flex-col divide-y divide-border-subtle/70">
                    <Row
                        ok={linked}
                        title={linked ? `已对接 Bot ${instance.link?.bot_id}` : '还没对接'}
                    >
                        {linked
                            ? 'Bot 侧的连接名是 ncd-app 开头的那条，在 Bot 的网络配置里能看到'
                            : '对接时桌面端同时写 Bot 侧的连接和 Koishi 侧的适配器条目'}
                    </Row>
                    <Row
                        ok={!!entry && entryOn}
                        title={
                            entry
                                ? `Koishi 侧 adapter-onebot:ncd-link ${entryOn ? '已启用' : '已停用'}`
                                : 'Koishi 侧适配器条目'
                        }
                    >
                        {entry ? (
                            <>
                                {selfId ? `selfId ${selfId}，` : ''}selfId 要和 Bot 的 QQ 号一致。
                                <button
                                    type="button"
                                    className="ml-1 inline-flex items-center gap-1 text-brand hover:underline"
                                    onClick={() => onGoTab('plugins')}
                                >
                                    <Puzzle size={11} />
                                    在插件页看
                                </button>
                            </>
                        ) : (
                            '对接时自动写好'
                        )}
                    </Row>
                </ul>
            </FormSection>

            <FormSection
                title="Bot 连过来的地址"
                description="跨机器对接时桌面端会换成 SSH 隧道的口，这里显示的是 Koishi 本机上的"
            >
                <CopyCodeBlock command={url} />
                <p className={cn('text-2xs text-text-tertiary')}>
                    路径 {KOISHI_REVERSE_WS_PATH} 是桌面端独占的；自己另接的 OneBot 账号用默认的
                    /onebot，互不影响
                </p>
            </FormSection>
        </ConfigForm>
    );
};
