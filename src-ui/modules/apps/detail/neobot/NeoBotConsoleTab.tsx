import { Button } from '../../../../shared/ui';
import type { AppInstance } from '../../../../core/ipc/types';
import { PanelCredentialCard } from './PanelCredentialCard';

export function NeoBotConsoleTab({
    instance,
    onOpenWebUi,
}: {
    instance: AppInstance;
    onOpenWebUi: () => void;
}) {
    return (
        <div className="flex flex-col gap-4">
            <PanelCredentialCard key={instance.id} instance={instance} onOpenWebUi={onOpenWebUi} />
            <div>
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={instance.state !== 'running'}
                    title={instance.state !== 'running' ? '启动实例后可打开控制台' : undefined}
                    onClick={onOpenWebUi}
                >
                    打开控制台
                </Button>
            </div>
        </div>
    );
}
