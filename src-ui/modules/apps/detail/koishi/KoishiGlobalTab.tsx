// 全局设置：koishi.yml 根上的键（指令前缀、昵称、国际化、延迟…），表单按上游 Context.Config 的 schema 画。
// 运行中保存会让 Koishi 按新设置重载一遍（worker 重拉），Bot 断开几秒后自己重连。

import { hydrateSchema } from '../../../../core/domain/apps/koishiSchema';
import { useKoishiPluginSchema } from '../../../../hooks/apps/useKoishiRuntime';
import { ConfigForm } from '../karin/configLayout';
import { PaneLoadError, PaneLoading } from '../PaneStatus';
import { Notice } from './Notice';
import { SchemasteryForm } from './SchemasteryForm';
import type { AppInstance, KoishiInstanceConfig } from '../../../../core/ipc/types';

export const KoishiGlobalTab: React.FC<{
    instance: AppInstance;
    config: KoishiInstanceConfig;
    onChange: (next: KoishiInstanceConfig) => void;
    disabled?: boolean;
}> = ({ instance, config, onChange, disabled }) => {
    const schema = useKoishiPluginSchema(instance.id, '');
    const node = hydrateSchema(schema.data?.schema);
    if (schema.isLoading) return <PaneLoading text="正在读取全局设置的表单…" />;
    if (schema.error || !node) {
        return (
            <PaneLoadError
                message={schema.data?.error ?? schema.error?.message ?? '读取全局设置的表单失败'}
                onRetry={() => void schema.refetch()}
            />
        );
    }
    return (
        <ConfigForm>
            {instance.state === 'running' && (
                <Notice title="运行中保存会让 Koishi 按新设置重载一遍，Bot 断开几秒后自己重连" />
            )}
            <SchemasteryForm
                schema={node}
                value={config.global}
                disabled={disabled}
                onChange={(global) => onChange({ ...config, global })}
            />
        </ConfigForm>
    );
};
