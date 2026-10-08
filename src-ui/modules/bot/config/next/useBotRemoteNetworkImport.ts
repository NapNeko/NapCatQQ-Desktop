// 远端 onebot 网络配置回读：读回来的 diff 先在对话框预览，用户确认才写进表单。
// 从配置页壳挪出；formData 经参数传入，确认后经 onApply 回写。

import { useState } from 'react';
import { pushInfoBar } from '../../../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../../../hooks/ui/pushErrorBar';
import { useRemoteNetworkPull } from '../../../../hooks/bot/useRemoteNetworkPull';
import { isRuntimeTargetConcreteRemote } from '../../../../core/domain/bot/runtime-target';
import {
    isPreviewEmpty,
    previewImportedNetwork,
    type ImportedNetworkPreview,
} from '../../../../core/domain/bot/imported-network';
import type { BotConfig } from '../../../../core/ipc/generated/domain/BotConfig';

interface RemoteNetworkImportDeps {
    botId: string | null;
    isEditMode: boolean;
    tourDemoMode: boolean;
    loadedConfig: BotConfig | null;
    formData: BotConfig;
    onApply: (next: BotConfig) => void;
}

export function useBotRemoteNetworkImport(deps: RemoteNetworkImportDeps) {
    const { botId, isEditMode, tourDemoMode, loadedConfig, formData, onApply } = deps;

    // 后端按已保存的 bot.json 找远端路径，所以看的是已保存的运行位置，不是表单里正在改的
    const canPullRemote =
        isEditMode &&
        !tourDemoMode &&
        loadedConfig != null &&
        isRuntimeTargetConcreteRemote(loadedConfig.bot.runtime_target);
    const { pullRemoteNetwork, pulling: pullingRemote } = useRemoteNetworkPull();
    const [remotePreview, setRemotePreview] = useState<ImportedNetworkPreview | null>(null);

    const handlePullRemote = async () => {
        if (!botId) return;
        try {
            const imported = await pullRemoteNetwork(botId);
            if (!imported) {
                pushInfoBar({
                    tone: 'info',
                    title: '远端还没有 onebot 配置文件',
                    content: '启动一次后桌面端会按当前配置写进去',
                    autoDismissMs: 4000,
                });
                return;
            }
            const preview = previewImportedNetwork(formData, imported);
            if (isPreviewEmpty(preview)) {
                pushInfoBar({
                    tone: 'success',
                    title: '和远端一致',
                    autoDismissMs: 3000,
                });
                return;
            }
            setRemotePreview(preview);
        } catch (e) {
            pushErrorBar({
                key: 'bot-remote-network',
                title: '读取远端配置失败',
                raw: String(e),
            });
        }
    };

    const confirmPullRemote = () => {
        if (remotePreview) onApply(remotePreview.next);
        setRemotePreview(null);
    };

    return {
        canPullRemote,
        handlePullRemote,
        pullingRemote,
        remotePreview,
        confirmPullRemote,
        setRemotePreview,
    };
}
