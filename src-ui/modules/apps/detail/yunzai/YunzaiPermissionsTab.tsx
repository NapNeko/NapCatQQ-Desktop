// 云崽「主人与权限」：other.yaml。主人在这里填和群里发 #设置主人 是一回事，省掉看日志抄验证码那一步。

import { FormSection, NumberField, StringListField, Switch, TextAreaField } from '../../../../shared/ui';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import type { YunzaiOtherConfig } from '../../../../core/ipc/types';
import type { YunzaiTabProps } from './YunzaiBasicTab';

/** 列表字段挂的第一条错误（后端按 `path/序号` 报） */
function listError(errors: Record<string, string>, path: string): string | undefined {
    const hit = Object.entries(errors).find(([k]) => k === path || k.startsWith(`${path}/`));
    return hit?.[1];
}

export const YunzaiPermissionsTab: React.FC<YunzaiTabProps> = ({ config, onChange, errors, disabled }) => {
    const other = config.other;
    const set = (patch: Partial<YunzaiOtherConfig>) => onChange({ ...config, other: { ...other, ...patch } });

    return (
        <ConfigForm>
            <FormSection title="主人">
                <div className={CONFIG_PAIR}>
                    <StringListField
                        label="主人 QQ"
                        hint="所有 Bot 都认"
                        value={other.master_qq}
                        error={listError(errors, 'other/master_qq')}
                        placeholder="QQ 号"
                        disabled={disabled}
                        onChange={(master_qq) => set({ master_qq })}
                    />
                    <StringListField
                        label="只对某个 Bot 的主人"
                        hint="写成 Bot号:主人号"
                        value={other.master}
                        error={listError(errors, 'other/master')}
                        placeholder="10001:12345"
                        disabled={disabled}
                        onChange={(master) => set({ master })}
                    />
                </div>
            </FormSection>

            <FormSection title="好友与群邀请">
                <div className="flex flex-wrap gap-x-8 gap-y-3">
                    <Switch
                        label="自动同意加好友"
                        checked={other.auto_friend === 1}
                        disabled={disabled}
                        onCheckedChange={(on) => set({ auto_friend: on ? 1 : 0 })}
                    />
                    <Switch
                        label="谁拉进群都同意"
                        hint="关着时只同意主人的邀请"
                        checked={other.auto_group === 1}
                        disabled={disabled}
                        onCheckedChange={(on) => set({ auto_group: on ? 1 : 0 })}
                    />
                </div>
                <NumberField
                    label="被拉进小群自动退出"
                    hint="群人数少于这个数就退，0 不退"
                    value={other.auto_quit}
                    min={0}
                    disabled={disabled}
                    className="max-w-xs"
                    onValueChange={(v) => set({ auto_quit: v ?? 0 })}
                />
            </FormSection>

            <FormSection title="私聊">
                <Switch
                    label="禁用私聊功能"
                    hint="主人不受限；开着时私聊只收 cookie、抽卡链接这类"
                    checked={other.disable_private}
                    disabled={disabled}
                    onCheckedChange={(disable_private) => set({ disable_private })}
                />
                {other.disable_private && (
                    <div className="flex flex-col gap-4">
                        <TextAreaField
                            label="禁用时回复"
                            value={other.disable_msg}
                            disabled={disabled}
                            onValueChange={(disable_msg) => set({ disable_msg })}
                        />
                        <StringListField
                            label="仍放行的关键字"
                            value={other.disable_adopt}
                            mono={false}
                            disabled={disabled}
                            onChange={(disable_adopt) => set({ disable_adopt })}
                        />
                    </div>
                )}
            </FormSection>

            <FormSection title="黑白名单" description="白名单非空时只理白名单里的">
                <div className={CONFIG_PAIR}>
                    <StringListField
                        label="白名单群"
                        value={other.white_group}
                        error={listError(errors, 'other/white_group')}
                        placeholder="群号"
                        disabled={disabled}
                        onChange={(white_group) => set({ white_group })}
                    />
                    <StringListField
                        label="白名单用户"
                        value={other.white_user}
                        error={listError(errors, 'other/white_user')}
                        placeholder="QQ 号"
                        disabled={disabled}
                        onChange={(white_user) => set({ white_user })}
                    />
                    <StringListField
                        label="黑名单群"
                        value={other.black_group}
                        error={listError(errors, 'other/black_group')}
                        placeholder="群号"
                        disabled={disabled}
                        onChange={(black_group) => set({ black_group })}
                    />
                    <StringListField
                        label="黑名单用户"
                        value={other.black_user}
                        error={listError(errors, 'other/black_user')}
                        placeholder="QQ 号"
                        disabled={disabled}
                        onChange={(black_user) => set({ black_user })}
                    />
                </div>
            </FormSection>
        </ConfigForm>
    );
};
