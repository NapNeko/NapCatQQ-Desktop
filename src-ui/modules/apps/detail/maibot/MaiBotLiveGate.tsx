// 资源页的运行门：这些数据在麦麦自己的库里、由运行中的麦麦给，没连上时整页换成这里，
// 不摆一排点不动的按钮。连上了返回 null，页面照常渲染。

import { KeyRound, Play, Power } from 'lucide-react';
import { Button, PagePlaceholder, Spinner } from '../../../../shared/ui';
import type { MaiBotRuntimeStatus } from '../../../../core/ipc/types';

export function maibotLive(status: MaiBotRuntimeStatus | undefined): boolean {
    return status?.gate === 'ok';
}

export const MaiBotLiveGate: React.FC<{
    status: MaiBotRuntimeStatus | undefined;
    /** 这页管的东西，拼进说明：「表情包在麦麦运行时才能看和改」 */
    what: string;
    onStart?: () => void;
    starting?: boolean;
}> = ({ status, what, onStart, starting }) => {
    if (maibotLive(status)) return null;

    if (!status || status.gate === 'unreachable') {
        return (
            <PagePlaceholder>
                <Spinner size="lg" tone="brand" label="正在连接麦麦" />
                <p className="text-sm text-text-secondary">正在等麦麦的 WebUI 起来</p>
                {status?.message && (
                    <p className="max-w-sm text-xs text-text-tertiary">{status.message}</p>
                )}
            </PagePlaceholder>
        );
    }

    const auth = status.gate === 'auth';
    const Icon = auth ? KeyRound : Power;
    return (
        <PagePlaceholder>
            <span
                className={
                    auth
                        ? 'flex h-11 w-11 items-center justify-center rounded-lg bg-warning-soft text-warning'
                        : 'flex h-11 w-11 items-center justify-center rounded-lg bg-inset text-text-tertiary'
                }
            >
                <Icon size={20} strokeWidth={1.9} />
            </span>
            <div className="flex flex-col gap-1">
                <p className="text-sm font-medium text-text">
                    {auth ? '麦麦的 WebUI 不认这个 token' : '麦麦没在运行'}
                </p>
                <p className="max-w-sm text-xs leading-relaxed text-text-tertiary">
                    {auth ? status.message : `${what}在麦麦运行时才能看和改`}
                </p>
            </div>
            {!auth && onStart && (
                <Button size="sm" variant="primary" disabled={starting} onClick={onStart}>
                    {starting ? <Spinner size="xs" className="text-white" /> : <Play size={13} />}
                    启动麦麦
                </Button>
            )}
        </PagePlaceholder>
    );
};
