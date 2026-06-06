// 关于 Tab。仅展示桌面端版本与许可。

import { APP_VERSION_LABEL } from '../../../core/domain/app-meta';
import { FieldRow, SettingsSection, SettingsTabSections } from '../_shared';

export function AboutTab() {
    const version = APP_VERSION_LABEL.replace(/^v/, '');

    return (
        <SettingsTabSections>
            <SettingsSection title="NapCatQQ Desktop">
            <FieldRow label="版本" description="当前安装的桌面端">
                <span className="font-mono text-[12.5px] tabular-nums text-text-secondary">
                    {version}
                </span>
            </FieldRow>

            <FieldRow
                label="许可"
                description="本项目以 GPL-3.0 协议开源，详见仓库 LICENSE"
                isLast
            >
                <span className="font-mono text-[12px] text-text-tertiary">GPL-3.0</span>
            </FieldRow>
            </SettingsSection>
        </SettingsTabSections>
    );
}