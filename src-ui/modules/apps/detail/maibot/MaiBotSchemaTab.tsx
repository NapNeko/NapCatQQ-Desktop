// 照 maibotPages 里的定义铺一页 bot_config：每个小节一个分组，高级字段默认收起，
// 页头一个开关放出来（开关状态整个详情页共用，切页不复位）。

import type { ReactNode } from 'react';
import { ExternalLink } from 'lucide-react';
import { Button, FormSection, Switch } from '../../../../shared/ui';
import { ConfigForm } from '../karin/configLayout';
import type { MaiBotChatSession, MaiBotInstanceConfig } from '../../../../core/ipc/types';
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
    /** 页顶的说明或运行状态 */
    intro?: ReactNode;
    /** 麦麦在跑时它见过的聊天，按聊天配的列表能直接挑 */
    chatTargets?: readonly MaiBotChatSession[];
    /** WebUI 应答了才能开 links 里的页 */
    live: boolean;
    onOpenWebUi: (path: string) => void;
}> = ({
    page,
    config,
    onChange,
    errors,
    disabled,
    showAdvanced,
    onShowAdvanced,
    intro,
    chatTargets,
    live,
    onOpenWebUi,
}) => {
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
        chatTargets,
    };
    const sections = page.sections.map((s) => ({ def: s, node: sectionNode(s) }));
    const anyAdvanced = sections.some(({ def, node }) => hasAdvanced(node, def.path, advanced));

    const links = page.links ?? [];
    return (
        <ConfigForm>
            {(links.length > 0 || anyAdvanced) && (
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <div className="flex min-w-0 flex-wrap items-center gap-1">
                        {links.map((l) => (
                            <Button
                                key={l.path}
                                size="sm"
                                variant="ghost"
                                disabled={!live}
                                title={live ? `在麦麦的 WebUI 里看${l.label}` : '启动麦麦后能看'}
                                onClick={() => onOpenWebUi(l.path)}
                            >
                                <ExternalLink size={13} />
                                {l.label}
                            </Button>
                        ))}
                    </div>
                    {anyAdvanced && (
                        <Switch
                            label="显示高级选项"
                            checked={showAdvanced}
                            onCheckedChange={onShowAdvanced}
                        />
                    )}
                </div>
            )}
            {intro}
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
