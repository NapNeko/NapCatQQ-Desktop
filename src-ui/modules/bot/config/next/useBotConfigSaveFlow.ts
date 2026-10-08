// 保存链路：兜底 name → 校验 → docker/runtime 闸门 → drift 检测 → 落盘。
// 从配置页壳挪出；services 调用（drift 检测）由白名单主文件注入，这里不直连。

import { useCallback, useState } from 'react';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../../../hooks/ui/pushErrorBar';
import {
    validateBotConfig,
    defaultStatusCommandConfig,
} from '../../../../core/domain/bot/config-defaults';
import type { BotConfig } from '../../../../core/ipc/generated/domain/BotConfig';
import type { ConfigDrift } from '../../../../core/ipc/generated/ConfigDrift';
import type { DriftDecision } from '../../../../core/ipc/generated/DriftDecision';

interface SaveFlowDeps {
    isEditMode: boolean;
    botId: string | null;
    tourDemoMode: boolean;
    formData: BotConfig;
    dockerSaveBlock: (config: BotConfig) => string | null;
    runtimeSaveBlock: (config: BotConfig) => string | null;
    /** 全局 WebUI 配置与 pristine 不同才写盘，失败抛给调用方统一报错。 */
    commitSnowlumaIfDirty: () => Promise<void>;
    /** botService.detectConfigDrift，主文件注入，保持 services 依赖留在白名单内。 */
    detectDrift: (botId: string) => Promise<ConfigDrift | null>;
    save: (config: BotConfig) => void;
    saveWithDecisions: (config: BotConfig, decisions: DriftDecision[]) => void;
}

export function useBotConfigSaveFlow(deps: SaveFlowDeps) {
    const {
        isEditMode,
        botId,
        tourDemoMode,
        formData,
        dockerSaveBlock,
        runtimeSaveBlock,
        commitSnowlumaIfDirty,
        detectDrift,
        save,
        saveWithDecisions,
    } = deps;

    const [pendingSaveDrift, setPendingSaveDrift] = useState<ConfigDrift | null>(null);
    const [pendingSaveData, setPendingSaveData] = useState<BotConfig | null>(null);

    const pushBlock = (title: string, content: string, key: string) => {
        pushInfoBar({ tone: 'danger', title, content, key });
    };

    const writeSnowlumaOrFail = async (): Promise<boolean> => {
        try {
            await commitSnowlumaIfDirty();
            return true;
        } catch (e) {
            pushErrorBar({
                key: 'bot-config-error',
                title: '保存失败',
                raw: `全局 WebUI 配置写入失败：${String(e)}`,
            });
            return false;
        }
    };

    const handleSave = async () => {
        if (tourDemoMode) {
            pushInfoBar({
                tone: 'info',
                title: '演示模式：不会真正添加',
                content: '这是入门引导里的演示新建，配置不会写入。结束引导后可自己点加号真实创建。',
                key: 'bot-tour-demo-save',
                autoDismissMs: 4000,
            });
            return;
        }

        // 实例名为空时用 placeholder 兜底(后端不允许空 name)
        const finalData: BotConfig = {
            ...formData,
            bot: {
                ...formData.bot,
                name: formData.bot.name.trim() || `Bot-${String(formData.bot.QQID).slice(-4)}`,
            },
            statusCommand:
                formData.bot.backend_type === 'snowluma'
                    ? (formData.statusCommand ?? defaultStatusCommandConfig())
                    : formData.statusCommand,
        };

        const validation = validateBotConfig(finalData);
        if (!validation.ok) {
            pushBlock('配置不通过', validation.reason, 'bot-config-error');
            return;
        }

        const dockerBlock = dockerSaveBlock(finalData);
        if (dockerBlock) {
            pushBlock('无法保存', dockerBlock, 'bot-config-docker-gate');
            return;
        }

        const runtimeBlock = runtimeSaveBlock(finalData);
        if (runtimeBlock) {
            pushBlock('无法保存', runtimeBlock, 'bot-config-runtime-gate');
            return;
        }

        // 先 drift 检测(纯读)。有 drift 就弹 dialog 等用户抉择,这之前绝不写任何
        // 后端配置——否则用户在 dialog 上点取消,SnowLuma 全局配置却已落盘且无法回滚。
        if (isEditMode && botId) {
            try {
                const drift = await detectDrift(botId);
                if (drift && (drift.added.length > 0 || drift.modified.length > 0)) {
                    setPendingSaveData(finalData);
                    setPendingSaveDrift(drift);
                    return;
                }
            } catch {
                // 检测失败不阻塞保存,继续走无 drift 分支
            }
        }

        // 无 drift(或新建模式):此时才提交 SnowLuma 全局配置并保存 Bot。
        if (!(await writeSnowlumaOrFail())) return;
        save(finalData);
    };

    const handleSaveDriftConfirm = useCallback(
        async (decisions: DriftDecision[]) => {
            if (!pendingSaveData) return;
            const dockerBlock = dockerSaveBlock(pendingSaveData);
            if (dockerBlock) {
                setPendingSaveDrift(null);
                setPendingSaveData(null);
                pushBlock('无法保存', dockerBlock, 'bot-config-docker-gate');
                return;
            }
            const runtimeBlock = runtimeSaveBlock(pendingSaveData);
            if (runtimeBlock) {
                setPendingSaveDrift(null);
                setPendingSaveData(null);
                pushBlock('无法保存', runtimeBlock, 'bot-config-runtime-gate');
                return;
            }
            if (!(await writeSnowlumaOrFail())) return;
            setPendingSaveDrift(null);
            saveWithDecisions(pendingSaveData, decisions);
            setPendingSaveData(null);
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [
            pendingSaveData,
            saveWithDecisions,
            commitSnowlumaIfDirty,
            dockerSaveBlock,
            runtimeSaveBlock,
        ],
    );

    const handleSaveDriftCancel = useCallback(() => {
        setPendingSaveDrift(null);
        setPendingSaveData(null);
    }, []);

    return { handleSave, pendingSaveDrift, handleSaveDriftConfirm, handleSaveDriftCancel };
}
