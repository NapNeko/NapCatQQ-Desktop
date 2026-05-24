// Bot flavor（NapCat / SnowLuma）派生工具，纯函数。

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { BackendType } from '../../ipc/generated/domain/BackendType';

export type Flavor = BackendType; // 'napcat' | 'snowluma'

export function flavorOf(config: BotConfig | null | undefined): Flavor | null {
    if (!config) return null;
    return config.bot.backend_type;
}

export function isSnowLumaFlavor(flavor: Flavor | null | undefined): boolean {
    return flavor === 'snowluma';
}
