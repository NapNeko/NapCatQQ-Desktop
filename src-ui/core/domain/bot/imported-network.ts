// 把远端 onebot 反向解析结果合进导入用的 BotConfig。纯函数，方便单测。

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { ImportedNetworkConfig } from '../../ipc/generated/domain/ImportedNetworkConfig';

export function applyImportedNetwork(
    cfg: BotConfig,
    imported: ImportedNetworkConfig,
): void {
    cfg.connect = imported.connect;
    if (imported.musicSignUrl) {
        cfg.bot = { ...cfg.bot, musicSignUrl: imported.musicSignUrl };
    }
    if (imported.statusCommand) {
        cfg.statusCommand = imported.statusCommand;
    }
    if (
        imported.enableLocalFile2Url != null ||
        imported.parseMultMsg != null
    ) {
        cfg.advanced = {
            ...cfg.advanced,
            ...(imported.enableLocalFile2Url != null
                ? { enableLocalFile2Url: imported.enableLocalFile2Url }
                : {}),
            ...(imported.parseMultMsg != null
                ? { parseMultMsg: imported.parseMultMsg }
                : {}),
        };
    }
}
