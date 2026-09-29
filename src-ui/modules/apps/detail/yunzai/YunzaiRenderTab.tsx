// 云崽「渲染」：喵喵面板这类图片怎么出。shotium 不要浏览器但不跑模板里的脚本；
// puppeteer 要浏览器：装时下了 Chrome 就不用填路径，没下就填本机 Chrome / Edge。

import { FormSection, NumberField, Select, TextField } from '../../../../shared/ui';
import { YUNZAI_RENDERERS } from '../../../../core/domain/apps/yunzaiConfig';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import type { YunzaiBotConfig } from '../../../../core/ipc/types';
import type { YunzaiTabProps } from './YunzaiBasicTab';

export const YunzaiRenderTab: React.FC<YunzaiTabProps> = ({ config, onChange, errors, disabled }) => {
    const bot = config.bot;
    const setBot = (patch: Partial<YunzaiBotConfig>) => onChange({ ...config, bot: { ...bot, ...patch } });

    return (
        <ConfigForm>
            <FormSection title="渲染后端">
                <Select
                    label="用哪个"
                    hint="自动：模板里有脚本走 puppeteer，没有走 shotium"
                    error={errors['renderer/name']}
                    items={YUNZAI_RENDERERS}
                    value={config.renderer.name}
                    disabled={disabled}
                    onValueChange={(name) => onChange({ ...config, renderer: { name } })}
                />
            </FormSection>

            <FormSection title="puppeteer 浏览器">
                <TextField
                    label="浏览器路径"
                    hint="留空用装云崽时下载的 Chrome"
                    placeholder="C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
                    value={bot.chromium_path}
                    disabled={disabled}
                    className="font-mono"
                    onValueChange={(chromium_path) => setBot({ chromium_path })}
                />
                <div className={CONFIG_PAIR}>
                    <TextField
                        label="远程浏览器地址"
                        hint="连单独跑着的 Chromium（如 browserless），填了就不在本机起浏览器"
                        placeholder="ws://127.0.0.1:3000"
                        value={bot.puppeteer_ws}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(puppeteer_ws) => setBot({ puppeteer_ws })}
                    />
                    <NumberField
                        label="截图超时（毫秒）"
                        placeholder="渲染器默认"
                        value={bot.puppeteer_timeout ?? null}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => setBot({ puppeteer_timeout: v ?? undefined })}
                    />
                </div>
            </FormSection>
        </ConfigForm>
    );
};
