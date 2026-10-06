// 知识库：麦麦的长期记忆，分导入 / 浏览 / 图谱三页。长期记忆默认是关的，关着、正在加载、
// 加载失败各给一句话和下一步，不给一排灰按钮。

import { AlertTriangle, Brain } from 'lucide-react';
import { Button, Spinner } from '../../../../shared/ui';
import type { AppInstance, MaiBotRuntimeStatus } from '../../../../core/ipc/types';
import { useMaiBotMemoryStatus } from '../../../../hooks/apps/useMaiBotMemory';
import { MaiBotLiveGate, maibotLive } from './MaiBotLiveGate';
import { KnowledgeBrowse } from './maibotKnowledgeBrowse';
import { KnowledgeGraph } from './maibotKnowledgeGraph';
import { KnowledgeImport } from './maibotKnowledgeImport';
import { KnowledgeSwitch, type KnowledgeView } from './maibotKnowledgeParts';

const Gate: React.FC<{
    icon: typeof Brain;
    title: string;
    text: string;
    action?: React.ReactNode;
}> = ({ icon: Icon, title, text, action }) => (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
        <span className="mb-1 inline-flex h-11 w-11 items-center justify-center rounded-full bg-inset text-text-tertiary">
            <Icon size={20} />
        </span>
        <p className="text-sm font-medium text-text">{title}</p>
        <p className="max-w-md text-xs leading-relaxed text-text-tertiary">{text}</p>
        {action && <div className="mt-2">{action}</div>}
    </div>
);

export const MaiBotKnowledgeTab: React.FC<{
    instance: AppInstance;
    status: MaiBotRuntimeStatus | undefined;
    view: KnowledgeView;
    onView: (v: KnowledgeView) => void;
    onGoTab: (tab: string) => void;
    onStart: () => void;
    starting: boolean;
}> = ({ instance, status, view, onView, onGoTab, onStart, starting }) => {
    const live = maibotLive(status);
    const mem = useMaiBotMemoryStatus(instance.id, live);

    if (!live)
        return (
            <MaiBotLiveGate status={status} what="知识库" onStart={onStart} starting={starting} />
        );
    const goMemory = (
        <Button size="sm" variant="primary" onClick={() => onGoTab('memory')}>
            去「记忆」页
        </Button>
    );
    if (!mem.data) {
        return mem.isError ? (
            <Gate
                icon={AlertTriangle}
                title="没问到长期记忆的状态"
                text={mem.error.message}
                action={
                    <Button size="sm" variant="secondary" onClick={() => void mem.refetch()}>
                        再试一次
                    </Button>
                }
            />
        ) : (
            <div className="flex flex-1 items-center justify-center">
                <Spinner size="md" tone="brand" label="正在读取" />
            </div>
        );
    }
    const s = mem.data;
    if (s.state === 'disabled') {
        return (
            <Gate
                icon={Brain}
                title="长期记忆没开"
                text="知识库就是麦麦的长期记忆，默认是关的。在「记忆」页打开并保存，不用重启，加载好了这里就能用。"
                action={goMemory}
            />
        );
    }
    if (s.state === 'starting') {
        return (
            <Gate
                icon={Brain}
                title="长期记忆正在加载"
                text="第一次打开要建索引，库大的话要等一会儿；好了这页会自己换过来。"
                action={<Spinner size="md" tone="brand" label="正在加载" />}
            />
        );
    }
    if (s.state === 'failed') {
        return (
            <Gate
                icon={AlertTriangle}
                title="长期记忆没加载起来"
                text={s.message || '看看「记忆」页里的嵌入模型配置，或者麦麦的日志。'}
                action={goMemory}
            />
        );
    }

    const switcher = <KnowledgeSwitch view={view} onView={onView} />;
    return (
        <div className="flex min-h-0 flex-1 flex-col">
            {s.notes.length > 0 && (
                <p className="mb-3 flex items-start gap-1.5 rounded-sm bg-warning-soft/60 px-3 py-2 text-xs text-text-secondary">
                    <AlertTriangle size={13} className="mt-px shrink-0 text-warning" />
                    {s.notes.join('；')}
                </p>
            )}
            {view === 'import' ? (
                <KnowledgeImport instanceId={instance.id} switcher={switcher} />
            ) : view === 'browse' ? (
                <KnowledgeBrowse
                    instanceId={instance.id}
                    switcher={switcher}
                    onImport={() => onView('import')}
                />
            ) : (
                <KnowledgeGraph
                    instanceId={instance.id}
                    switcher={switcher}
                    onImport={() => onView('import')}
                />
            )}
        </div>
    );
};
