// 照 maibotPages 里的定义铺一页 bot_config：每个小节一个分组，高级字段默认收起，
// 页头一个开关放出来（开关状态整个详情页共用，切页不复位）。

import type { ReactNode } from 'react';
import { FormSection, Switch } from '../../../../shared/ui';
import { ConfigForm } from '../karin/configLayout';
import type { MaiBotInstanceConfig } from '../../../../core/ipc/types';
import { anyVisible, FieldGrid, hasAdvanced, type SchemaCtx } from './SchemaForm';
import { BOT_SCHEMA, nodeAt, type UiNode } from '../../../../core/domain/apps/maibotSchema';
import type { SchemaPageDef, SchemaSectionDef } from './maibotPages';

function sectionNode(s: SchemaSectionDef): UiNode | undefined {
    const node = nodeAt(BOT_SCHEMA, s.path);
    if (!node || !s.fields) return node;
    return { ...node, fields: node.fields.filter((f) => s.fields?.includes(f.name)) };
}

export const MaiBotSchemaTab: React.FC<{
    page: SchemaPageDef;
    config: MaiBotInstanceConfig;
    onChange: (next: MaiBotInstanceConfig) => void;
    errors: Record<string, string>;
    disabled?: boolean;
    showAdvanced: boolean;
    onShowAdvanced: (next: boolean) => void;
    /** 页顶的说明或提示条 */
    intro?: ReactNode;
}> = ({ page, config, onChange, errors, disabled, showAdvanced, onShowAdvanced, intro }) => {
    const skip = new Set(page.skip);
    const advanced = new Set(page.advanced);
    const ctx: SchemaCtx = {
        value: config.bot,
        onChange: (bot) => onChange({ ...config, bot: bot as MaiBotInstanceConfig['bot'] }),
        errors,
        prefix: 'bot',
        disabled,
        showAdvanced,
        skip,
        advanced,
    };
    const sections = page.sections.map((s) => ({ def: s, node: sectionNode(s) }));
    const anyAdvanced = sections.some(({ def, node }) => hasAdvanced(node, def.path, advanced));

    return (
        <ConfigForm>
            {(intro || anyAdvanced) && (
                <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">{intro}</div>
                    {anyAdvanced && (
                        <Switch
                            label="显示高级选项"
                            checked={showAdvanced}
                            onCheckedChange={onShowAdvanced}
                        />
                    )}
                </div>
            )}
            {sections.map(({ def, node }) =>
                node && anyVisible(ctx, def.path, node) ? (
                    <FormSection key={def.path.join('.')} title={def.title} description={def.description}>
                        <FieldGrid ctx={ctx} path={def.path} node={node} />
                    </FormSection>
                ) : null,
            )}
        </ConfigForm>
    );
};
