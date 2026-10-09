import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { ModelsEditor } from './NeoBotModelsEditor';
import { PanelPage, type NeoBotPageProps } from './workspaceParts';

export function NeoBotModelsTab(props: NeoBotPageProps) {
    const query = usePanelJson(props.instanceId, 'models', '/api/config/models', asRecord);
    return (
        <PanelPage query={query} onGoTab={props.onGoTab}>
            {(doc) => <ModelsEditor key={props.instanceId} {...props} doc={doc} />}
        </PanelPage>
    );
}
