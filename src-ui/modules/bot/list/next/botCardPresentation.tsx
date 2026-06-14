// 列表卡：从领域层 re-export，避免在 UI 目录重复状态规则。

export type {
    StatusBadgeSpec as BotBadgeSpec,
    BotListCardStatus,
} from '../../../../core/domain/bot/bot-status-presentation';

export {
    botProcessBadge as botLifecycleBadge,
    buildBotListCardStatus,
    botListCardMetaLine,
} from '../../../../core/domain/bot/bot-status-presentation';