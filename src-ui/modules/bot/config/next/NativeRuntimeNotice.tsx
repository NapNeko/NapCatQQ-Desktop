// 只有直接运行区域挂载时才订阅组件就绪状态，探测刷新不影响宿主选择控件。
import {
    useComponentNames,
    useRuntimeReadiness,
} from '../../../../hooks/components/useRuntimeReadiness';
import { describeBlocking } from '../../../../core/domain/components/readiness';
import type { BackendType } from '../../../../core/ipc/generated/domain/BackendType';
import { ExpandPresence } from '../../../../shared/ui/motion/ExpandPresence';
import { RuntimeNotice } from './RuntimeNotice';

export function NativeRuntimeNotice({
    hostId,
    backend,
    transportFailed,
}: {
    hostId: string | null;
    backend: BackendType;
    transportFailed: boolean;
}) {
    const { readiness, probing } = useRuntimeReadiness(hostId, backend);
    const names = useComponentNames();
    const local = hostId === 'local';
    const blocking = readiness ? describeBlocking(readiness, names) : null;
    const message = transportFailed
        ? '远端主机不可达，保存后无法启动。请先在「远程主机」页恢复连接。'
        : blocking
          ? local
              ? `本机${blocking}，请到「组件」页安装后再使用本机直接运行`
              : `远程主机${blocking}，请到「组件」页安装`
          : local && probing
            ? '正在检测本机运行时组件…'
            : null;
    return (
        <ExpandPresence visible={hostId != null && !!message}>
            <div className="pt-3">
                <RuntimeNotice tone={transportFailed ? 'danger' : blocking ? 'warn' : 'neutral'}>
                    {message}
                </RuntimeNotice>
            </div>
        </ExpandPresence>
    );
}
