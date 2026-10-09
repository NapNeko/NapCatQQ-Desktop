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
            <PanelCredentialCard instance={instance} onOpenWebUi={onOpenWebUi} />
            <div>
                <Button
                    size="sm"
                    variant="secondary"
                    disabled={instance.state === 'installing'}
                    onClick={onOpenWebUi}
                >
                    打开上游 WebUI
                </Button>
            </div>
        </div>
    );
}
