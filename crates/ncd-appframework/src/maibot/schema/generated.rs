// 由 scripts/maibot/codegen.py 从 MaiBot bot_config 8.14.40 / model_config 1.17.9 生成，别手改；升级上游后重跑。
// 字段名、类型照上游 pydantic 类；默认值不在这里，读取时垫 defaults/ 下的上游默认文件。

#![allow(clippy::derivable_impls)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::IssueSink;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotBotConfig {
    /// 适配器没有上报身份时使用的备用主平台，例如 qq。
    pub platform: String,
    /// 适配器没有上报 QQ 身份时使用的备用账号 ID。
    pub qq_account: String,
    /// 其他平台的备用账号，格式为 platform:账号；适配器身份存在时不参与判断。
    pub platforms: Vec<String>,
    /// 麦麦显示和自称时使用的名字。
    pub nickname: String,
    /// 别人可能用来称呼麦麦的名字，用于辅助识别提及。
    pub alias_names: Vec<String>,
}

impl MaiBotBotConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotBotConfig {
    fn default() -> Self {
        Self {
            platform: String::from(""),
            qq_account: String::from(""),
            platforms: vec![],
            nickname: String::from("麦麦"),
            alias_names: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotPersonalityConfig {
    /// 麦麦的人格和身份设定，建议简短描述她是谁、是什么性格。
    pub personality: String,
    /// Planner 使用的行动准则，例如何时参与聊天、如何观察局面以及何时保持安静。
    pub behavior_style: String,
    /// 麦麦平时说话的风格，例如简短、温和、吐槽或正式。
    pub reply_style: String,
    /// 备用说话风格；触发后只影响本次回复。
    pub multiple_reply_style: Vec<String>,
    /// 随机启用备用风格的概率；0 表示不随机切换。
    pub multiple_probability: f64,
}

impl MaiBotPersonalityConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/multiple_probability"), self.multiple_probability, Some(0.0), Some(1.0));
    }
}

impl Default for MaiBotPersonalityConfig {
    fn default() -> Self {
        Self {
            personality: String::from("是一个大二女大学生，现在正在上网和群友聊天。善于用人类的角度思考问题，聊天偏日常。"),
            behavior_style: String::from("是大二女大学生，现在正在上网和群友聊天。善于用人类的角度思考问题，聊天偏日常。不会没话题硬找话题，"),
            reply_style: String::from("你的风格平淡简短，可以参考贴吧的回复风格。不滥用比喻或者生硬句子。视情况省略主语或者进行倒装，风格较为随意。"),
            multiple_reply_style: vec![String::from("你的风格平淡但不失讽刺，很简短,很白话。可以参考贴吧，微博的回复风格。"), String::from("用1-2个字进行回复"), String::from("用1-2个符号进行回复"), String::from("言辭凝練古雅，穿插《論語》經句卻不晦澀，以文言短句為基，輔以淺白語意，持長者溫和風範，全用繁體字表達，具先秦儒者談吐韻致。"), String::from("带点翻译腔，但不要太长")],
            multiple_probability: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotTalkRulesItem {
    /// 规则作用的平台；留空表示不限定平台，* 表示任意平台。
    pub platform: String,
    /// 规则作用的群号或用户 ID；留空表示不限定聊天，* 表示任意聊天。
    pub item_id: String,
    /// 规则作用于群聊还是私聊。
    #[ts(type = "\"group\" | \"private\"")]
    pub rule_type: String,
    /// 规则生效时间；留空为兜底，* 为全天，也可填 23:00-02:00。
    pub time: String,
    /// 该规则下的发言频率；0 更安静，1 按正常频率。
    pub value: f64,
}

impl MaiBotTalkRulesItem {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/rule_type"), &self.rule_type, &["group", "private"]);
    }
}

impl Default for MaiBotTalkRulesItem {
    fn default() -> Self {
        Self {
            platform: String::from(""),
            item_id: String::from(""),
            rule_type: String::from("group"),
            time: String::from(""),
            value: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotChatReplyTimingConfig {
    /// 群聊里麦麦主动说话的频率；越小越安静。
    pub talk_value: f64,
    /// 私聊里麦麦主动说话的频率；越小越安静。
    pub private_talk_value: f64,
    /// 开启后，只要消息提到麦麦名字就更容易回复。
    pub mentioned_bot_reply: bool,
    /// 开启后，被 @ 时会尽量回复。
    pub inevitable_at_reply: bool,
    /// 控制新消息何时进入 Planner。
    #[ts(type = "\"frequency\" | \"reply_necessity\"")]
    pub reply_trigger_mode: String,
    /// 思考时来了新消息，最多重新思考多少次。
    #[ts(type = "number")]
    pub planner_interrupt_max_consecutive_count: i64,
    /// Planner 最多连续调用 wait 多少次；达到上限后 wait 工具会拒绝继续进入等待。
    #[ts(type = "number")]
    pub max_consecutive_wait_count: i64,
    /// 连续决定不回复后，下一次检查前先等多久。
    pub no_action_backoff_base_seconds: f64,
    /// 不回复退避等待的最长时间。
    pub no_action_backoff_cap_seconds: f64,
    /// 连续几次不回复后开始放慢检查。
    #[ts(type = "number")]
    pub no_action_backoff_start_count: i64,
    /// 等待期间新消息达到多少条就立刻重新处理；0 表示不按条数打断等待。
    #[ts(type = "number")]
    pub no_action_backoff_bypass_pending_count: i64,
    /// 开启后，可以按聊天或时间段单独调整发言频率。
    pub enable_talk_value_rules: bool,
    /// 动态发言频率规则；可让麦麦在某些群、私聊或时段更活跃或更安静。
    pub talk_value_rules: Vec<MaiBotTalkRulesItem>,
}

impl MaiBotChatReplyTimingConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/talk_value"), self.talk_value, Some(0.0), Some(1.0));
        sink.range(&format!("{path}/private_talk_value"), self.private_talk_value, Some(0.0), Some(1.0));
        sink.one_of(&format!("{path}/reply_trigger_mode"), &self.reply_trigger_mode, &["frequency", "reply_necessity"]);
        sink.range(&format!("{path}/planner_interrupt_max_consecutive_count"), self.planner_interrupt_max_consecutive_count as f64, Some(0.0), None);
        sink.range(&format!("{path}/max_consecutive_wait_count"), self.max_consecutive_wait_count as f64, Some(1.0), None);
        sink.range(&format!("{path}/no_action_backoff_base_seconds"), self.no_action_backoff_base_seconds, Some(0.0), None);
        sink.range(&format!("{path}/no_action_backoff_cap_seconds"), self.no_action_backoff_cap_seconds, Some(0.0), None);
        sink.range(&format!("{path}/no_action_backoff_start_count"), self.no_action_backoff_start_count as f64, Some(1.0), None);
        sink.range(&format!("{path}/no_action_backoff_bypass_pending_count"), self.no_action_backoff_bypass_pending_count as f64, Some(0.0), None);
        for (i, item) in self.talk_value_rules.iter().enumerate() {
            item.validate(&format!("{path}/talk_value_rules/{i}"), sink);
        }
    }
}

impl Default for MaiBotChatReplyTimingConfig {
    fn default() -> Self {
        Self {
            talk_value: 1.0,
            private_talk_value: 1.0,
            mentioned_bot_reply: false,
            inevitable_at_reply: true,
            reply_trigger_mode: String::from("frequency"),
            planner_interrupt_max_consecutive_count: 0,
            max_consecutive_wait_count: 3,
            no_action_backoff_base_seconds: 15.0,
            no_action_backoff_cap_seconds: 300.0,
            no_action_backoff_start_count: 2,
            no_action_backoff_bypass_pending_count: 6,
            enable_talk_value_rules: false,
            talk_value_rules: vec![MaiBotTalkRulesItem { platform: String::from(""), item_id: String::from(""), rule_type: String::from("group"), time: String::from("00:00-08:59"), value: 0.8 }, MaiBotTalkRulesItem { platform: String::from(""), item_id: String::from(""), rule_type: String::from("group"), time: String::from("09:00-18:59"), value: 1.0 }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotExtraPromptItem {
    /// 额外提示作用的平台，和聊天流 ID、提示内容需要一起填写。
    pub platform: String,
    /// 额外提示作用的群号或用户 ID。
    pub item_id: String,
    /// 额外提示作用于群聊还是私聊。
    #[ts(type = "\"group\" | \"private\"")]
    pub rule_type: String,
    /// 给这个聊天额外补充的要求。
    pub prompt: String,
}

impl MaiBotExtraPromptItem {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/rule_type"), &self.rule_type, &["group", "private"]);
    }
}

impl Default for MaiBotExtraPromptItem {
    fn default() -> Self {
        Self {
            platform: String::from(""),
            item_id: String::from(""),
            rule_type: String::from("group"),
            prompt: String::from(""),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotChatReplyStyleConfig {
    /// 回复时是否可以引用上一条或相关消息。
    pub enable_reply_quote: bool,
    /// 群聊通用提示词，告诉麦麦群聊中该怎么说话。
    pub group_chat_prompt: String,
    /// 私聊通用提示词，告诉麦麦私聊中该怎么说话。
    pub private_chat_prompts: String,
    /// 给指定群聊或私聊额外补充聊天要求；有特殊群规或语气要求时再加。
    pub chat_prompts: Vec<MaiBotExtraPromptItem>,
}

impl MaiBotChatReplyStyleConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        for (i, item) in self.chat_prompts.iter().enumerate() {
            item.validate(&format!("{path}/chat_prompts/{i}"), sink);
        }
    }
}

impl Default for MaiBotChatReplyStyleConfig {
    fn default() -> Self {
        Self {
            enable_reply_quote: true,
            group_chat_prompt: String::from("你正在qq群里聊天，下面是群里正在聊的内容，聊天中包含文字，图片和表情包等消息。\n回复尽量简短一些。最好一次对一个话题进行回复，但必须考虑不同群友发言之间的交互，免得啰嗦或者回复内容太乱。请注意把握聊天内容。\n不要总是提及自己的身份背景，根据聊天内容自由发挥，但是要日常不浮夸，不要刻意找话题。\n不用刻意回复其他人发送的表情包，只要关注表情包表达的含义。你可以适当发送表情包表达情绪。控制回复的频率，不要每个人的消息都回复，优先回复你感兴趣的或者主动提及你的，适当回复其他话题。\n"),
            private_chat_prompts: String::from("你正在聊天，下面是正在聊的内容，其中包含聊天记录和聊天中的图片。\n回复尽量简短一些。请注意把握聊天内容。\n请考虑对方的发言频率，想法，思考自己何时回复以及回复内容。\n"),
            chat_prompts: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotChatConfig {
    /// 群聊回复时参考的最近消息数量；越大越懂上下文，也更耗模型。
    #[ts(type = "number")]
    pub max_context_size: i64,
    /// 私聊回复时参考的最近消息数量。
    #[ts(type = "number")]
    pub max_private_context_size: i64,
    /// 压缩部分上下文，减少模型消耗；一般建议开启。
    pub enable_context_optimization: bool,
    /// 打开后会主动召回最近聊天发生的事情。
    pub mid_term_memory: bool,
    /// 最多保留多少条聊天回想；设为 0 表示不保留。
    #[ts(type = "number")]
    pub mid_term_memory_lenth: i64,
    /// 什么时候回复、回复频率与等待退避配置。
    pub reply_timing: MaiBotChatReplyTimingConfig,
    /// 如何回复、引用回复与聊天 Prompt 配置。
    pub reply_style: MaiBotChatReplyStyleConfig,
}

impl MaiBotChatConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/mid_term_memory_lenth"), self.mid_term_memory_lenth as f64, Some(0.0), None);
        self.reply_timing.validate(&format!("{path}/reply_timing"), sink);
        self.reply_style.validate(&format!("{path}/reply_style"), sink);
    }
}

impl Default for MaiBotChatConfig {
    fn default() -> Self {
        Self {
            max_context_size: 40,
            max_private_context_size: 60,
            enable_context_optimization: true,
            mid_term_memory: true,
            mid_term_memory_lenth: 10,
            reply_timing: MaiBotChatReplyTimingConfig { talk_value: 1.0, private_talk_value: 1.0, mentioned_bot_reply: false, inevitable_at_reply: true, reply_trigger_mode: String::from("frequency"), planner_interrupt_max_consecutive_count: 0, max_consecutive_wait_count: 3, no_action_backoff_base_seconds: 15.0, no_action_backoff_cap_seconds: 300.0, no_action_backoff_start_count: 2, no_action_backoff_bypass_pending_count: 6, enable_talk_value_rules: false, talk_value_rules: vec![MaiBotTalkRulesItem { platform: String::from(""), item_id: String::from(""), rule_type: String::from("group"), time: String::from("00:00-08:59"), value: 0.8 }, MaiBotTalkRulesItem { platform: String::from(""), item_id: String::from(""), rule_type: String::from("group"), time: String::from("09:00-18:59"), value: 1.0 }] },
            reply_style: MaiBotChatReplyStyleConfig { enable_reply_quote: true, group_chat_prompt: String::from("你正在qq群里聊天，下面是群里正在聊的内容，聊天中包含文字，图片和表情包等消息。\n回复尽量简短一些。最好一次对一个话题进行回复，但必须考虑不同群友发言之间的交互，免得啰嗦或者回复内容太乱。请注意把握聊天内容。\n不要总是提及自己的身份背景，根据聊天内容自由发挥，但是要日常不浮夸，不要刻意找话题。\n不用刻意回复其他人发送的表情包，只要关注表情包表达的含义。你可以适当发送表情包表达情绪。控制回复的频率，不要每个人的消息都回复，优先回复你感兴趣的或者主动提及你的，适当回复其他话题。\n"), private_chat_prompts: String::from("你正在聊天，下面是正在聊的内容，其中包含聊天记录和聊天中的图片。\n回复尽量简短一些。请注意把握聊天内容。\n请考虑对方的发言频率，想法，思考自己何时回复以及回复内容。\n"), chat_prompts: vec![] },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAttentionDriftConfig {
    /// 开启后，麦麦会更容易被有趣的新话题、梗或反差点吸引，但仍需保持上下文可理解。
    pub enabled: bool,
    /// 控制注意力漂移的整体表现档位，而不是用数值概率描述。
    #[ts(type = "\"subtle\" | \"active\" | \"scattered\" | \"wild\"")]
    pub drift_level: String,
    /// 控制话题漂移后需要多强地回到当前聊天上下文。
    #[ts(type = "\"strict\" | \"balanced\" | \"loose\"")]
    pub anchor_policy: String,
    /// 控制短句、吐槽、语气词等短反应在漂移风格中的使用方式。
    #[ts(type = "\"reserved\" | \"natural\" | \"lively\"")]
    pub reaction_style: String,
}

impl MaiBotAttentionDriftConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/drift_level"), &self.drift_level, &["subtle", "active", "scattered", "wild"]);
        sink.one_of(&format!("{path}/anchor_policy"), &self.anchor_policy, &["strict", "balanced", "loose"]);
        sink.one_of(&format!("{path}/reaction_style"), &self.reaction_style, &["reserved", "natural", "lively"]);
    }
}

impl Default for MaiBotAttentionDriftConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            drift_level: String::from("scattered"),
            anchor_policy: String::from("balanced"),
            reaction_style: String::from("lively"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotLearningItem {
    /// 平台，与ID一起留空表示全局
    pub platform: String,
    /// 要单独配置的群号或用户 ID；留空表示默认规则。
    pub item_id: String,
    /// 这条规则作用于群聊还是私聊。
    #[ts(type = "\"group\" | \"private\"")]
    pub r#type: String,
    /// 是否在这个聊天里使用已学到的内容。
    pub r#use: bool,
    /// 是否从这个聊天里继续学习新内容。
    pub learn: bool,
}

impl MaiBotLearningItem {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/type"), &self.r#type, &["group", "private"]);
    }
}

impl Default for MaiBotLearningItem {
    fn default() -> Self {
        Self {
            platform: String::from(""),
            item_id: String::from(""),
            r#type: String::from("group"),
            r#use: true,
            learn: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotTargetItem {
    /// 要单独配置的平台；和聊天流 ID 都留空表示全局默认，仅平台有值且聊天流 ID 留空表示平台兜底。
    pub platform: String,
    /// 用户/群 ID；留空时和平台字段共同决定全局默认或平台兜底，* 表示任意聊天流。
    pub item_id: String,
    /// 聊天流类型，group（群聊）或private（私聊）
    #[ts(type = "\"group\" | \"private\"")]
    pub rule_type: String,
}

impl MaiBotTargetItem {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/rule_type"), &self.rule_type, &["group", "private"]);
    }
}

impl Default for MaiBotTargetItem {
    fn default() -> Self {
        Self {
            platform: String::from(""),
            item_id: String::from(""),
            rule_type: String::from("group"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotChatStreamGroup {
    /// 这个组里的聊天流会共享对应的学习内容。
    pub targets: Vec<MaiBotTargetItem>,
}

impl MaiBotChatStreamGroup {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        for (i, item) in self.targets.iter().enumerate() {
            item.validate(&format!("{path}/targets/{i}"), sink);
        }
    }
}

impl Default for MaiBotChatStreamGroup {
    fn default() -> Self {
        Self {
            targets: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotExperimentalConfig {
    /// 让麦麦从聊天中学习什么时候该怎么回应的经验。
    pub enable_behavior_learning: bool,
    /// 开启后，reply 动作可通过 attach_pic、attach_emoji、attach_at 参数附加图片、表情包或 at。
    pub enable_rich_reply: bool,
    /// 实验性人格情绪特点；理性冷静和多愁善感会追加人格后缀，中性不追加内容。
    #[ts(type = "\"rational_calm\" | \"neutral\" | \"sentimental\"")]
    pub emotion_trait: String,
    /// 注意力漂移实验模式；让麦麦在群聊/私聊中表现出更活跃的联想和轻微话题漂移。
    pub attention_drift: MaiBotAttentionDriftConfig,
    /// 配置哪些聊天会学习和使用行为经验；默认规则不够时再单独添加。
    pub behavior_learning_list: Vec<MaiBotLearningItem>,
    /// 让多个群聊或私聊共享学到的行为经验。
    pub behavior_groups: Vec<MaiBotChatStreamGroup>,
    /// 让麦麦同一时间只专注一个聊天流，适合直播或高强度聊天场景。
    pub focus_mode: bool,
    /// Focus 模式是否也作用于私聊。
    pub focus_on_private: bool,
    /// Focus 白名单。配置后只有命中的群聊或私聊会进入 Focus；留空表示所有符合聊天类型开关的聊天都可进入 Focus。
    pub focus_chat_whitelist: Vec<MaiBotTargetItem>,
    /// 把聊天流分组后，同组共享 Focus，不同组互不抢占。
    pub focus_groups: Vec<MaiBotChatStreamGroup>,
    /// 当前关注的聊天多久没继续处理后，允许被其他聊天唤醒。
    #[ts(type = "number")]
    pub focus_cool_time: i64,
}

impl MaiBotExperimentalConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/emotion_trait"), &self.emotion_trait, &["rational_calm", "neutral", "sentimental"]);
        self.attention_drift.validate(&format!("{path}/attention_drift"), sink);
        for (i, item) in self.behavior_learning_list.iter().enumerate() {
            item.validate(&format!("{path}/behavior_learning_list/{i}"), sink);
        }
        for (i, item) in self.behavior_groups.iter().enumerate() {
            item.validate(&format!("{path}/behavior_groups/{i}"), sink);
        }
        for (i, item) in self.focus_chat_whitelist.iter().enumerate() {
            item.validate(&format!("{path}/focus_chat_whitelist/{i}"), sink);
        }
        for (i, item) in self.focus_groups.iter().enumerate() {
            item.validate(&format!("{path}/focus_groups/{i}"), sink);
        }
        sink.range(&format!("{path}/focus_cool_time"), self.focus_cool_time as f64, Some(1.0), None);
    }
}

impl Default for MaiBotExperimentalConfig {
    fn default() -> Self {
        Self {
            enable_behavior_learning: false,
            enable_rich_reply: false,
            emotion_trait: String::from("neutral"),
            attention_drift: MaiBotAttentionDriftConfig { enabled: false, drift_level: String::from("scattered"), anchor_policy: String::from("balanced"), reaction_style: String::from("lively") },
            behavior_learning_list: vec![MaiBotLearningItem { platform: String::from(""), item_id: String::from(""), r#type: String::from("group"), r#use: true, learn: true }],
            behavior_groups: vec![],
            focus_mode: false,
            focus_on_private: false,
            focus_chat_whitelist: vec![],
            focus_groups: vec![],
            focus_cool_time: 120,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotImageCacheCleanupConfig {
    /// 开启后会自动删除长期不用的图片缓存。
    pub enabled: bool,
    /// 每隔多少小时检查一次旧图片。
    pub check_interval_hours: f64,
    /// 图片文件多久没被使用后可以删除。
    #[ts(type = "number")]
    pub image_file_retention_days: i64,
    /// 图片文件删掉后，识别文字还能保留多久。
    #[ts(type = "number")]
    pub no_file_result_retention_days: i64,
}

impl MaiBotImageCacheCleanupConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/check_interval_hours"), self.check_interval_hours, Some(0.016666666666666666), None);
        sink.range(&format!("{path}/image_file_retention_days"), self.image_file_retention_days as f64, Some(1.0), None);
        sink.range(&format!("{path}/no_file_result_retention_days"), self.no_file_result_retention_days as f64, Some(1.0), None);
    }
}

impl Default for MaiBotImageCacheCleanupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_hours: 6.0,
            image_file_retention_days: 14,
            no_file_result_retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotVisualConfig {
    /// 控制规划阶段是否把图片内容直接发送给 planner 模型。auto 会根据模型是否支持视觉自动选择；text 始终只使用文字和图片识别结果；multimodal 会强制使用多模态输入。
    #[ts(type = "\"text\" | \"multimodal\" | \"auto\"")]
    pub planner_mode: String,
    /// 控制回复生成阶段是否把图片内容直接发送给 replyer 模型。auto 会根据模型是否支持视觉自动选择；text 始终只使用文字和图片识别结果；multimodal 会强制使用多模态输入。
    #[ts(type = "\"text\" | \"multimodal\" | \"auto\"")]
    pub replyer_mode: String,
    /// 一次多模态请求最多带多少张图，太大可能更慢更贵。
    #[ts(type = "number")]
    pub max_image_num: i64,
    /// 等图片识别完成的最长秒数；0 表示不等待。
    pub wait_image_recognize_max_time: f64,
    /// 收到太大的图片时，是否自动压缩或丢弃。
    pub handle_oversized_images: bool,
    /// 超过这个大小的图片会按过大图片处理方法处理；0 表示不限。
    pub max_image_size_mb: f64,
    /// 大图的处理方式：压缩后继续用，或直接丢弃。
    #[ts(type = "\"compress\" | \"discard\"")]
    pub oversized_image_handle_method: String,
    /// 定期清理旧图片缓存，减少磁盘占用。
    pub image_cache_cleanup: MaiBotImageCacheCleanupConfig,
}

impl MaiBotVisualConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/planner_mode"), &self.planner_mode, &["text", "multimodal", "auto"]);
        sink.one_of(&format!("{path}/replyer_mode"), &self.replyer_mode, &["text", "multimodal", "auto"]);
        sink.range(&format!("{path}/max_image_num"), self.max_image_num as f64, Some(0.0), None);
        sink.range(&format!("{path}/wait_image_recognize_max_time"), self.wait_image_recognize_max_time, Some(0.0), None);
        sink.range(&format!("{path}/max_image_size_mb"), self.max_image_size_mb, Some(0.0), None);
        sink.one_of(&format!("{path}/oversized_image_handle_method"), &self.oversized_image_handle_method, &["compress", "discard"]);
        self.image_cache_cleanup.validate(&format!("{path}/image_cache_cleanup"), sink);
    }
}

impl Default for MaiBotVisualConfig {
    fn default() -> Self {
        Self {
            planner_mode: String::from("auto"),
            replyer_mode: String::from("auto"),
            max_image_num: 128,
            wait_image_recognize_max_time: 10.0,
            handle_oversized_images: true,
            max_image_size_mb: 30.0,
            oversized_image_handle_method: String::from("compress"),
            image_cache_cleanup: MaiBotImageCacheCleanupConfig { enabled: true, check_interval_hours: 6.0, image_file_retention_days: 14, no_file_result_retention_days: 30 },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotExpressionConfig {
    /// 仅使用人工精选的表达。
    pub expression_checked_only: bool,
    /// 写入表达方式前先让 AI 检查，减少学到奇怪内容。
    pub expression_self_reflect: bool,
    /// 表达方式的使用策略：legacy 随手抽取候选，vector_intent 使用表达意图与嵌入召回。
    #[ts(type = "\"legacy\" | \"vector_intent\"")]
    pub expression_selection_mode: String,
    /// 向量召回使用的表达索引 JSON；相对路径按项目根目录解析。
    pub expression_vector_index_path: String,
    /// 向量召回后最多交给表达方式 LLM 选择的候选数；硬上限为 50。
    #[ts(type = "number")]
    pub expression_vector_candidate_pool_size: i64,
    /// 同时运行的表达学习任务数量；太高可能占用更多资源。
    #[ts(type = "number")]
    pub max_expression_learner: i64,
    /// 配置哪些聊天会学习和使用表达方式；默认规则不够时再单独添加。
    pub learning_list: Vec<MaiBotLearningItem>,
    /// 让多个群聊或私聊共享学到的表达方式。
    pub expression_groups: Vec<MaiBotChatStreamGroup>,
}

impl MaiBotExpressionConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/expression_selection_mode"), &self.expression_selection_mode, &["legacy", "vector_intent"]);
        sink.range(&format!("{path}/expression_vector_candidate_pool_size"), self.expression_vector_candidate_pool_size as f64, Some(1.0), Some(50.0));
        for (i, item) in self.learning_list.iter().enumerate() {
            item.validate(&format!("{path}/learning_list/{i}"), sink);
        }
        for (i, item) in self.expression_groups.iter().enumerate() {
            item.validate(&format!("{path}/expression_groups/{i}"), sink);
        }
    }
}

impl Default for MaiBotExpressionConfig {
    fn default() -> Self {
        Self {
            expression_checked_only: true,
            expression_self_reflect: true,
            expression_selection_mode: String::from("legacy"),
            expression_vector_index_path: String::from("data/expression_selection/expression_vector_index.json"),
            expression_vector_candidate_pool_size: 50,
            max_expression_learner: 3,
            learning_list: vec![MaiBotLearningItem { platform: String::from(""), item_id: String::from(""), r#type: String::from("group"), r#use: true, learn: true }],
            expression_groups: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotJargonConfig {
    /// 配置哪些聊天会学习和使用黑话；默认规则不够时再单独添加。
    pub learning_list: Vec<MaiBotLearningItem>,
    /// 让多个群聊或私聊共享学到的黑话。
    pub jargon_groups: Vec<MaiBotChatStreamGroup>,
}

impl MaiBotJargonConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        for (i, item) in self.learning_list.iter().enumerate() {
            item.validate(&format!("{path}/learning_list/{i}"), sink);
        }
        for (i, item) in self.jargon_groups.iter().enumerate() {
            item.validate(&format!("{path}/jargon_groups/{i}"), sink);
        }
    }
}

impl Default for MaiBotJargonConfig {
    fn default() -> Self {
        Self {
            learning_list: vec![MaiBotLearningItem { platform: String::from(""), item_id: String::from(""), r#type: String::from("group"), r#use: true, learn: true }],
            jargon_groups: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixPluginConfig {
    /// 是否启用长期记忆系统
    pub enabled: bool,
}

impl MaiBotAMemorixPluginConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotAMemorixPluginConfig {
    fn default() -> Self {
        Self {
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixIntegrationConfig {
    /// 是否允许麦麦在聊天时查询长期记忆
    pub enable_memory_query_tool: bool,
    /// 每次默认从长期记忆中取回多少条结果
    #[ts(type = "number")]
    pub memory_query_default_limit: i64,
    /// 是否允许麦麦查询人物画像记忆
    pub enable_person_profile_query_tool: bool,
    /// 是否在 Maisaka Planner 调用前自动注入当前对象相关的人物画像
    pub enable_person_profile_injection: bool,
    /// 每轮自动注入的人物画像数量上限
    #[ts(type = "number")]
    pub person_profile_injection_max_profiles: i64,
    /// 是否根据当前聊天印象自然拉起长期记忆
    pub heuristic_memory_recall_enabled: bool,
    pub heuristic_memory_cross_chat_enabled: bool,
    /// 生成当前聊天印象时使用的最近消息数量
    #[ts(type = "number")]
    pub heuristic_memory_recall_window_size: i64,
    /// 每轮自然拉起的长期记忆数量上限
    #[ts(type = "number")]
    pub heuristic_memory_recall_limit: i64,
    /// 自然拉起记忆注入文本的最大字符数
    #[ts(type = "number")]
    pub heuristic_memory_recall_max_chars: i64,
    /// 同一聊天流两次自然拉起的最小间隔秒数
    #[ts(type = "number")]
    pub heuristic_memory_recall_min_interval_seconds: i64,
    /// 两次自然拉起之间至少需要新增的当前聊天流消息数
    #[ts(type = "number")]
    pub heuristic_memory_recall_min_new_messages: i64,
    /// 同一聊天流自然拉起结果的运行时缓存时间
    #[ts(type = "number")]
    pub heuristic_memory_recall_cache_ttl_seconds: i64,
    pub heuristic_memory_group_to_private_enabled: bool,
    pub heuristic_memory_private_to_group_enabled: bool,
    /// 是否在发送回复后自动提取并写回人物事实到长期记忆
    pub person_fact_writeback_enabled: bool,
    /// 是否在 Maisaka 聊天过程中按消息窗口自动写回聊天摘要到长期记忆
    pub chat_summary_writeback_enabled: bool,
    /// 自动写回聊天摘要的消息窗口阈值
    #[ts(type = "number")]
    pub chat_summary_writeback_message_threshold: i64,
    /// 自动写回聊天摘要时，从聊天流中回看的消息条数
    #[ts(type = "number")]
    pub chat_summary_writeback_context_length: i64,
    /// 是否启用自然语言记忆修正的后台接口
    pub fuzzy_modify_enabled: bool,
    pub fuzzy_modify_auto_execute_enabled: bool,
    pub fuzzy_modify_confirm_threshold: f64,
    /// 每次记忆修正交给 LLM 的候选记忆上限
    #[ts(type = "number")]
    pub fuzzy_modify_candidate_limit: i64,
    #[ts(type = "number")]
    pub fuzzy_modify_max_targets: i64,
    pub fuzzy_modify_allow_global_scope: bool,
    pub feedback_correction_enabled: bool,
    /// 反馈窗口时长（小时），以 query_memory 执行时间为起点
    pub feedback_correction_window_hours: f64,
    #[ts(type = "number")]
    pub feedback_correction_check_interval_minutes: i64,
    #[ts(type = "number")]
    pub feedback_correction_batch_size: i64,
    pub feedback_correction_auto_apply_threshold: f64,
    /// 每个纠错任务最多使用的窗口内用户反馈消息数
    #[ts(type = "number")]
    pub feedback_correction_max_feedback_messages: i64,
    pub feedback_correction_prefilter_enabled: bool,
    pub feedback_correction_paragraph_mark_enabled: bool,
    pub feedback_correction_paragraph_hard_filter_enabled: bool,
    /// 是否在反馈纠错后将受影响人物画像加入刷新队列
    pub feedback_correction_profile_refresh_enabled: bool,
    pub feedback_correction_profile_force_refresh_on_read: bool,
    /// 是否在反馈纠错后将受影响 source 加入 episode 重建队列
    pub feedback_correction_episode_rebuild_enabled: bool,
    pub feedback_correction_episode_query_block_enabled: bool,
    #[ts(type = "number")]
    pub feedback_correction_reconcile_interval_minutes: i64,
    #[ts(type = "number")]
    pub feedback_correction_reconcile_batch_size: i64,
}

impl MaiBotAMemorixIntegrationConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/memory_query_default_limit"), self.memory_query_default_limit as f64, Some(1.0), Some(20.0));
        sink.range(&format!("{path}/person_profile_injection_max_profiles"), self.person_profile_injection_max_profiles as f64, Some(1.0), Some(5.0));
        sink.range(&format!("{path}/heuristic_memory_recall_window_size"), self.heuristic_memory_recall_window_size as f64, Some(1.0), Some(200.0));
        sink.range(&format!("{path}/heuristic_memory_recall_limit"), self.heuristic_memory_recall_limit as f64, Some(1.0), Some(10.0));
        sink.range(&format!("{path}/heuristic_memory_recall_max_chars"), self.heuristic_memory_recall_max_chars as f64, Some(100.0), Some(4000.0));
        sink.range(&format!("{path}/heuristic_memory_recall_min_interval_seconds"), self.heuristic_memory_recall_min_interval_seconds as f64, Some(0.0), None);
        sink.range(&format!("{path}/heuristic_memory_recall_min_new_messages"), self.heuristic_memory_recall_min_new_messages as f64, Some(1.0), None);
        sink.range(&format!("{path}/heuristic_memory_recall_cache_ttl_seconds"), self.heuristic_memory_recall_cache_ttl_seconds as f64, Some(0.0), None);
        sink.range(&format!("{path}/chat_summary_writeback_message_threshold"), self.chat_summary_writeback_message_threshold as f64, Some(1.0), None);
        sink.range(&format!("{path}/chat_summary_writeback_context_length"), self.chat_summary_writeback_context_length as f64, Some(1.0), Some(500.0));
        sink.range(&format!("{path}/fuzzy_modify_candidate_limit"), self.fuzzy_modify_candidate_limit as f64, Some(1.0), Some(100.0));
        sink.range(&format!("{path}/feedback_correction_window_hours"), self.feedback_correction_window_hours, Some(0.1), None);
        sink.range(&format!("{path}/feedback_correction_max_feedback_messages"), self.feedback_correction_max_feedback_messages as f64, Some(1.0), Some(200.0));
    }
}

impl Default for MaiBotAMemorixIntegrationConfig {
    fn default() -> Self {
        Self {
            enable_memory_query_tool: true,
            memory_query_default_limit: 5,
            enable_person_profile_query_tool: true,
            enable_person_profile_injection: true,
            person_profile_injection_max_profiles: 3,
            heuristic_memory_recall_enabled: false,
            heuristic_memory_cross_chat_enabled: false,
            heuristic_memory_recall_window_size: 20,
            heuristic_memory_recall_limit: 3,
            heuristic_memory_recall_max_chars: 900,
            heuristic_memory_recall_min_interval_seconds: 180,
            heuristic_memory_recall_min_new_messages: 60,
            heuristic_memory_recall_cache_ttl_seconds: 300,
            heuristic_memory_group_to_private_enabled: false,
            heuristic_memory_private_to_group_enabled: false,
            person_fact_writeback_enabled: true,
            chat_summary_writeback_enabled: true,
            chat_summary_writeback_message_threshold: 36,
            chat_summary_writeback_context_length: 36,
            fuzzy_modify_enabled: true,
            fuzzy_modify_auto_execute_enabled: false,
            fuzzy_modify_confirm_threshold: 0.85,
            fuzzy_modify_candidate_limit: 20,
            fuzzy_modify_max_targets: 5,
            fuzzy_modify_allow_global_scope: false,
            feedback_correction_enabled: false,
            feedback_correction_window_hours: 12.0,
            feedback_correction_check_interval_minutes: 30,
            feedback_correction_batch_size: 20,
            feedback_correction_auto_apply_threshold: 0.85,
            feedback_correction_max_feedback_messages: 30,
            feedback_correction_prefilter_enabled: true,
            feedback_correction_paragraph_mark_enabled: true,
            feedback_correction_paragraph_hard_filter_enabled: true,
            feedback_correction_profile_refresh_enabled: true,
            feedback_correction_profile_force_refresh_on_read: true,
            feedback_correction_episode_rebuild_enabled: true,
            feedback_correction_episode_query_block_enabled: true,
            feedback_correction_reconcile_interval_minutes: 5,
            feedback_correction_reconcile_batch_size: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixStorageConfig {
    pub data_dir: String,
}

impl MaiBotAMemorixStorageConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotAMemorixStorageConfig {
    fn default() -> Self {
        Self {
            data_dir: String::from("data/a-memorix"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixEmbeddingFallbackConfig {
    /// 是否启用回退机制
    pub enabled: bool,
    /// 探测间隔秒数
    #[ts(type = "number")]
    pub probe_interval_seconds: i64,
    pub allow_metadata_only_write: bool,
}

impl MaiBotAMemorixEmbeddingFallbackConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/probe_interval_seconds"), self.probe_interval_seconds as f64, Some(10.0), None);
    }
}

impl Default for MaiBotAMemorixEmbeddingFallbackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            probe_interval_seconds: 180,
            allow_metadata_only_write: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixParagraphVectorBackfillConfig {
    /// 是否启用回填任务
    pub enabled: bool,
    #[ts(type = "number")]
    pub interval_seconds: i64,
    /// 单批回填数量
    #[ts(type = "number")]
    pub batch_size: i64,
    /// 最大重试次数
    #[ts(type = "number")]
    pub max_retry: i64,
}

impl MaiBotAMemorixParagraphVectorBackfillConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/batch_size"), self.batch_size as f64, Some(1.0), None);
        sink.range(&format!("{path}/max_retry"), self.max_retry as f64, Some(0.0), None);
    }
}

impl Default for MaiBotAMemorixParagraphVectorBackfillConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 60,
            batch_size: 64,
            max_retry: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixEmbeddingConfig {
    /// 用于把记忆内容转换成向量的模型，auto 表示自动选择
    pub model_name: String,
    #[ts(type = "number")]
    pub dimension: i64,
    /// 是否在 embedding 请求中携带维度参数：explicit 仅显式指定时携带，always 总是携带，never 不携带
    #[ts(type = "\"explicit\" | \"always\" | \"never\"")]
    pub dimension_request_mode: String,
    /// 每次向量化请求处理的记忆条数
    #[ts(type = "number")]
    pub batch_size: i64,
    /// 同时进行的向量化请求数量
    #[ts(type = "number")]
    pub max_concurrent: i64,
    /// 是否缓存向量化结果
    pub enable_cache: bool,
    #[ts(type = "number")]
    pub runtime_train_threshold: i64,
    #[ts(type = "\"int8\"")]
    pub quantization_type: String,
    /// Embedding 回退配置
    pub fallback: MaiBotAMemorixEmbeddingFallbackConfig,
    /// 段落向量回填配置
    pub paragraph_vector_backfill: MaiBotAMemorixParagraphVectorBackfillConfig,
}

impl MaiBotAMemorixEmbeddingConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/dimension_request_mode"), &self.dimension_request_mode, &["explicit", "always", "never"]);
        sink.range(&format!("{path}/batch_size"), self.batch_size as f64, Some(1.0), None);
        sink.range(&format!("{path}/max_concurrent"), self.max_concurrent as f64, Some(1.0), None);
        sink.one_of(&format!("{path}/quantization_type"), &self.quantization_type, &["int8"]);
        self.fallback.validate(&format!("{path}/fallback"), sink);
        self.paragraph_vector_backfill.validate(&format!("{path}/paragraph_vector_backfill"), sink);
    }
}

impl Default for MaiBotAMemorixEmbeddingConfig {
    fn default() -> Self {
        Self {
            model_name: String::from("auto"),
            dimension: 1024,
            dimension_request_mode: String::from("explicit"),
            batch_size: 32,
            max_concurrent: 5,
            enable_cache: false,
            runtime_train_threshold: 256,
            quantization_type: String::from("int8"),
            fallback: MaiBotAMemorixEmbeddingFallbackConfig { enabled: true, probe_interval_seconds: 180, allow_metadata_only_write: true },
            paragraph_vector_backfill: MaiBotAMemorixParagraphVectorBackfillConfig { enabled: true, interval_seconds: 60, batch_size: 64, max_retry: 5 },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixSmartFallbackConfig {
    /// 是否启用智能兜底检索
    pub enabled: bool,
}

impl MaiBotAMemorixSmartFallbackConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotAMemorixSmartFallbackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixRetrievalSearchConfig {
    /// 智能兜底检索配置
    pub smart_fallback: MaiBotAMemorixSmartFallbackConfig,
}

impl MaiBotAMemorixRetrievalSearchConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        self.smart_fallback.validate(&format!("{path}/smart_fallback"), sink);
    }
}

impl Default for MaiBotAMemorixRetrievalSearchConfig {
    fn default() -> Self {
        Self {
            smart_fallback: MaiBotAMemorixSmartFallbackConfig { enabled: true },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixFusionRetrievalConfig {
    #[ts(type = "\"weighted_rrf\" | \"alpha_legacy\"")]
    pub method: String,
    #[ts(type = "number")]
    pub rrf_k: i64,
    pub vector_weight: f64,
    pub bm25_weight: f64,
}

impl MaiBotAMemorixFusionRetrievalConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/method"), &self.method, &["weighted_rrf", "alpha_legacy"]);
    }
}

impl Default for MaiBotAMemorixFusionRetrievalConfig {
    fn default() -> Self {
        Self {
            method: String::from("weighted_rrf"),
            rrf_k: 60,
            vector_weight: 0.7,
            bm25_weight: 0.3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixRelationVectorizationConfig {
    pub enabled: bool,
    pub backfill_enabled: bool,
    /// 导入时是否写入关系向量
    pub write_on_import: bool,
}

impl MaiBotAMemorixRelationVectorizationConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotAMemorixRelationVectorizationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backfill_enabled: false,
            write_on_import: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixRelationIntentVectorPoolConfig {
    /// 关系意图命中时的图谱池候选数
    #[ts(type = "number")]
    pub graph_top_k: i64,
    pub semantic_weight: f64,
    pub sparse_weight: f64,
    pub graph_weight: f64,
    pub return_relation_items: bool,
}

impl MaiBotAMemorixRelationIntentVectorPoolConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/graph_top_k"), self.graph_top_k as f64, Some(1.0), None);
    }
}

impl Default for MaiBotAMemorixRelationIntentVectorPoolConfig {
    fn default() -> Self {
        Self {
            graph_top_k: 80,
            semantic_weight: 0.45,
            sparse_weight: 0.15,
            graph_weight: 0.4,
            return_relation_items: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixVectorPoolsConfig {
    #[ts(type = "\"single\" | \"dual\"")]
    pub mode: String,
    /// 段落向量池候选数
    #[ts(type = "number")]
    pub paragraph_top_k: i64,
    /// 图谱向量池候选数
    #[ts(type = "number")]
    pub graph_top_k: i64,
    /// 图谱证据展开段落上限
    #[ts(type = "number")]
    pub graph_expand_paragraph_k: i64,
    #[ts(type = "number")]
    pub relation_expand_per_hit: i64,
    #[ts(type = "number")]
    pub entity_expand_per_hit: i64,
    pub relation_evidence_weight: f64,
    pub entity_evidence_weight: f64,
    pub semantic_weight: f64,
    pub sparse_weight: f64,
    pub graph_weight: f64,
    /// 关系意图命中时的双向量池配置
    pub relation_intent: MaiBotAMemorixRelationIntentVectorPoolConfig,
}

impl MaiBotAMemorixVectorPoolsConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/mode"), &self.mode, &["single", "dual"]);
        sink.range(&format!("{path}/paragraph_top_k"), self.paragraph_top_k as f64, Some(1.0), None);
        sink.range(&format!("{path}/graph_top_k"), self.graph_top_k as f64, Some(1.0), None);
        sink.range(&format!("{path}/graph_expand_paragraph_k"), self.graph_expand_paragraph_k as f64, Some(1.0), None);
        self.relation_intent.validate(&format!("{path}/relation_intent"), sink);
    }
}

impl Default for MaiBotAMemorixVectorPoolsConfig {
    fn default() -> Self {
        Self {
            mode: String::from("dual"),
            paragraph_top_k: 20,
            graph_top_k: 40,
            graph_expand_paragraph_k: 80,
            relation_expand_per_hit: 5,
            entity_expand_per_hit: 8,
            relation_evidence_weight: 1.0,
            entity_evidence_weight: 0.55,
            semantic_weight: 0.65,
            sparse_weight: 0.2,
            graph_weight: 0.15,
            relation_intent: MaiBotAMemorixRelationIntentVectorPoolConfig { graph_top_k: 80, semantic_weight: 0.45, sparse_weight: 0.15, graph_weight: 0.4, return_relation_items: false },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixSparseRetrievalConfig {
    /// 是否启用稀疏检索
    pub enabled: bool,
    #[ts(type = "\"fts5\"")]
    pub backend: String,
    /// 稀疏检索模式
    #[ts(type = "\"auto\" | \"fallback_only\" | \"hybrid\"")]
    pub mode: String,
    /// 分词模式
    #[ts(type = "\"jieba\" | \"mixed\" | \"char_2gram\"")]
    pub tokenizer_mode: String,
    /// 段落候选数
    #[ts(type = "number")]
    pub candidate_k: i64,
    /// 关系候选数
    #[ts(type = "number")]
    pub relation_candidate_k: i64,
}

impl MaiBotAMemorixSparseRetrievalConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/backend"), &self.backend, &["fts5"]);
        sink.one_of(&format!("{path}/mode"), &self.mode, &["auto", "fallback_only", "hybrid"]);
        sink.one_of(&format!("{path}/tokenizer_mode"), &self.tokenizer_mode, &["jieba", "mixed", "char_2gram"]);
        sink.range(&format!("{path}/candidate_k"), self.candidate_k as f64, Some(1.0), None);
        sink.range(&format!("{path}/relation_candidate_k"), self.relation_candidate_k as f64, Some(1.0), None);
    }
}

impl Default for MaiBotAMemorixSparseRetrievalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: String::from("fts5"),
            mode: String::from("auto"),
            tokenizer_mode: String::from("jieba"),
            candidate_k: 80,
            relation_candidate_k: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixRetrievalConfig {
    /// 段落候选数
    #[ts(type = "number")]
    pub top_k_paragraphs: i64,
    /// 关系候选数
    #[ts(type = "number")]
    pub top_k_relations: i64,
    /// 最终返回条数
    #[ts(type = "number")]
    pub top_k_final: i64,
    pub alpha: f64,
    /// 是否启用 PPR
    pub enable_ppr: bool,
    pub ppr_alpha: f64,
    /// PPR 超时秒数
    pub ppr_timeout_seconds: f64,
    #[ts(type = "number")]
    pub ppr_concurrency_limit: i64,
    /// 是否启用并行检索
    pub enable_parallel: bool,
    /// 搜索后处理配置
    pub search: MaiBotAMemorixRetrievalSearchConfig,
    pub fusion: MaiBotAMemorixFusionRetrievalConfig,
    /// 关系向量化配置
    pub relation_vectorization: MaiBotAMemorixRelationVectorizationConfig,
    /// 双向量池检索配置
    pub vector_pools: MaiBotAMemorixVectorPoolsConfig,
    /// 稀疏检索配置
    pub sparse: MaiBotAMemorixSparseRetrievalConfig,
}

impl MaiBotAMemorixRetrievalConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/top_k_paragraphs"), self.top_k_paragraphs as f64, Some(1.0), None);
        sink.range(&format!("{path}/top_k_relations"), self.top_k_relations as f64, Some(1.0), None);
        sink.range(&format!("{path}/top_k_final"), self.top_k_final as f64, Some(1.0), None);
        sink.range(&format!("{path}/ppr_timeout_seconds"), self.ppr_timeout_seconds, Some(0.1), None);
        self.search.validate(&format!("{path}/search"), sink);
        self.fusion.validate(&format!("{path}/fusion"), sink);
        self.relation_vectorization.validate(&format!("{path}/relation_vectorization"), sink);
        self.vector_pools.validate(&format!("{path}/vector_pools"), sink);
        self.sparse.validate(&format!("{path}/sparse"), sink);
    }
}

impl Default for MaiBotAMemorixRetrievalConfig {
    fn default() -> Self {
        Self {
            top_k_paragraphs: 20,
            top_k_relations: 10,
            top_k_final: 10,
            alpha: 0.5,
            enable_ppr: true,
            ppr_alpha: 0.85,
            ppr_timeout_seconds: 1.5,
            ppr_concurrency_limit: 4,
            enable_parallel: true,
            search: MaiBotAMemorixRetrievalSearchConfig { smart_fallback: MaiBotAMemorixSmartFallbackConfig { enabled: true } },
            fusion: MaiBotAMemorixFusionRetrievalConfig { method: String::from("weighted_rrf"), rrf_k: 60, vector_weight: 0.7, bm25_weight: 0.3 },
            relation_vectorization: MaiBotAMemorixRelationVectorizationConfig { enabled: false, backfill_enabled: false, write_on_import: true },
            vector_pools: MaiBotAMemorixVectorPoolsConfig { mode: String::from("dual"), paragraph_top_k: 20, graph_top_k: 40, graph_expand_paragraph_k: 80, relation_expand_per_hit: 5, entity_expand_per_hit: 8, relation_evidence_weight: 1.0, entity_evidence_weight: 0.55, semantic_weight: 0.65, sparse_weight: 0.2, graph_weight: 0.15, relation_intent: MaiBotAMemorixRelationIntentVectorPoolConfig { graph_top_k: 80, semantic_weight: 0.45, sparse_weight: 0.15, graph_weight: 0.4, return_relation_items: false } },
            sparse: MaiBotAMemorixSparseRetrievalConfig { enabled: true, backend: String::from("fts5"), mode: String::from("auto"), tokenizer_mode: String::from("jieba"), candidate_k: 80, relation_candidate_k: 60 },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixThresholdConfig {
    pub min_threshold: f64,
    pub max_threshold: f64,
    #[ts(type = "number")]
    pub percentile: i64,
    /// 最小保留条数
    #[ts(type = "number")]
    pub min_results: i64,
}

impl MaiBotAMemorixThresholdConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/min_results"), self.min_results as f64, Some(1.0), None);
    }
}

impl Default for MaiBotAMemorixThresholdConfig {
    fn default() -> Self {
        Self {
            min_threshold: 0.29,
            max_threshold: 0.95,
            percentile: 75,
            min_results: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixRetrievalSubtypeFilterConfig {
    /// 是否启用当前检索结果类型的跨聊天流过滤
    pub enabled: bool,
    /// 过滤模式
    #[ts(type = "\"blacklist\" | \"whitelist\"")]
    pub mode: String,
    /// 聊天流列表
    pub chats: Vec<String>,
}

impl MaiBotAMemorixRetrievalSubtypeFilterConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/mode"), &self.mode, &["blacklist", "whitelist"]);
    }
}

impl Default for MaiBotAMemorixRetrievalSubtypeFilterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: String::from("blacklist"),
            chats: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixRetrievalFilterConfig {
    /// 普通 paragraph/relation 命中的跨聊天流检索后置过滤
    pub chat_stream: MaiBotAMemorixRetrievalSubtypeFilterConfig,
    /// 聊天总结命中的跨聊天流检索后置过滤
    pub chat_summary: MaiBotAMemorixRetrievalSubtypeFilterConfig,
    /// Episode 命中的跨聊天流检索后置过滤
    pub episode: MaiBotAMemorixRetrievalSubtypeFilterConfig,
}

impl MaiBotAMemorixRetrievalFilterConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        self.chat_stream.validate(&format!("{path}/chat_stream"), sink);
        self.chat_summary.validate(&format!("{path}/chat_summary"), sink);
        self.episode.validate(&format!("{path}/episode"), sink);
    }
}

impl Default for MaiBotAMemorixRetrievalFilterConfig {
    fn default() -> Self {
        Self {
            chat_stream: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] },
            chat_summary: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] },
            episode: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixFilterConfig {
    /// 是否启用聊天过滤
    pub enabled: bool,
    /// 过滤模式
    #[ts(type = "\"blacklist\" | \"whitelist\"")]
    pub mode: String,
    /// 聊天流列表
    pub chats: Vec<String>,
    /// 仅对跨聊天流检索结果生效的分类型过滤，不影响本聊天流读取自身记忆、写入和后台生成
    pub retrieval: MaiBotAMemorixRetrievalFilterConfig,
}

impl MaiBotAMemorixFilterConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/mode"), &self.mode, &["blacklist", "whitelist"]);
        self.retrieval.validate(&format!("{path}/retrieval"), sink);
    }
}

impl Default for MaiBotAMemorixFilterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: String::from("blacklist"),
            chats: vec![],
            retrieval: MaiBotAMemorixRetrievalFilterConfig { chat_stream: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] }, chat_summary: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] }, episode: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] } },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixEpisodeConfig {
    /// 是否启用 Episode
    pub enabled: bool,
    /// 是否启用自动生成
    pub generation_enabled: bool,
    /// 来源级 Episode 任务轮询间隔秒数
    pub source_poll_interval_seconds: f64,
    /// 单轮领取的来源任务数
    #[ts(type = "number")]
    pub source_batch_size: i64,
    /// 每个来源版本的最大尝试次数，包含首次尝试
    #[ts(type = "number")]
    pub source_max_retry: i64,
    /// 来源任务租约时长秒数
    pub source_lease_seconds: f64,
    /// 来源持续写入时允许的最大防抖等待秒数
    pub source_max_wait_seconds: f64,
    /// 单次最大段落数
    #[ts(type = "number")]
    pub max_paragraphs_per_call: i64,
    /// 单次最大字符数
    #[ts(type = "number")]
    pub max_chars_per_call: i64,
    /// 时间窗口小时数
    pub source_time_window_hours: f64,
    /// 分段模型选择
    pub segmentation_model: String,
    /// 自动生成 Episode 时跳过的来源类型
    pub disabled_source_types: Vec<String>,
}

impl MaiBotAMemorixEpisodeConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/source_poll_interval_seconds"), self.source_poll_interval_seconds, Some(0.1), None);
        sink.range(&format!("{path}/source_batch_size"), self.source_batch_size as f64, Some(1.0), None);
        sink.range(&format!("{path}/source_max_retry"), self.source_max_retry as f64, Some(1.0), None);
        sink.range(&format!("{path}/source_lease_seconds"), self.source_lease_seconds, Some(1.0), None);
        sink.range(&format!("{path}/source_max_wait_seconds"), self.source_max_wait_seconds, Some(0.0), None);
        sink.range(&format!("{path}/max_paragraphs_per_call"), self.max_paragraphs_per_call as f64, Some(1.0), None);
        sink.range(&format!("{path}/max_chars_per_call"), self.max_chars_per_call as f64, Some(100.0), None);
        sink.range(&format!("{path}/source_time_window_hours"), self.source_time_window_hours, Some(0.0), None);
    }
}

impl Default for MaiBotAMemorixEpisodeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            generation_enabled: true,
            source_poll_interval_seconds: 1.0,
            source_batch_size: 20,
            source_max_retry: 3,
            source_lease_seconds: 1800.0,
            source_max_wait_seconds: 60.0,
            max_paragraphs_per_call: 20,
            max_chars_per_call: 6000,
            source_time_window_hours: 24.0,
            segmentation_model: String::from("auto"),
            disabled_source_types: vec![String::from("person_fact")],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixPersonProfileConfig {
    /// 是否启用画像
    pub enabled: bool,
    /// 刷新间隔分钟数
    #[ts(type = "number")]
    pub refresh_interval_minutes: i64,
    /// 活跃窗口小时数
    pub active_window_hours: f64,
    /// 单轮最大刷新数
    #[ts(type = "number")]
    pub max_refresh_per_cycle: i64,
    #[ts(type = "number")]
    pub refresh_debounce_seconds: i64,
    #[ts(type = "number")]
    pub refresh_queue_interval_seconds: i64,
    #[ts(type = "number")]
    pub refresh_queue_batch_size: i64,
    #[ts(type = "number")]
    pub refresh_retry_backoff_seconds: i64,
    #[ts(type = "number")]
    pub max_retry: i64,
    /// 证据条数
    #[ts(type = "number")]
    pub top_k_evidence: i64,
    #[ts(type = "number")]
    pub evidence_classification_max_tokens: i64,
    pub evidence_classification_temperature: f64,
}

impl MaiBotAMemorixPersonProfileConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/refresh_interval_minutes"), self.refresh_interval_minutes as f64, Some(1.0), None);
        sink.range(&format!("{path}/active_window_hours"), self.active_window_hours, Some(1.0), None);
        sink.range(&format!("{path}/max_refresh_per_cycle"), self.max_refresh_per_cycle as f64, Some(1.0), None);
        sink.range(&format!("{path}/top_k_evidence"), self.top_k_evidence as f64, Some(1.0), None);
    }
}

impl Default for MaiBotAMemorixPersonProfileConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_interval_minutes: 30,
            active_window_hours: 72.0,
            max_refresh_per_cycle: 50,
            refresh_debounce_seconds: 120,
            refresh_queue_interval_seconds: 60,
            refresh_queue_batch_size: 10,
            refresh_retry_backoff_seconds: 300,
            max_retry: 3,
            top_k_evidence: 12,
            evidence_classification_max_tokens: 1200,
            evidence_classification_temperature: 0.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixMemoryEvolutionConfig {
    /// 是否启用记忆演化
    pub enabled: bool,
    /// 半衰期小时数
    pub half_life_hours: f64,
    /// 裁剪阈值
    pub prune_threshold: f64,
    /// 冻结时长小时数
    pub freeze_duration_hours: f64,
    /// 冻结关系恢复为活跃状态的保留强度阈值
    pub revive_threshold: f64,
    /// 记忆被最终采用时的饱和加强系数
    pub access_reinforcement_alpha: f64,
    /// 同一关系两次访问加强之间的最短分钟数，0表示不限制
    pub access_reinforcement_cooldown_minutes: f64,
    /// 用户显式加强或独立新证据的饱和加强系数
    pub explicit_reinforcement_alpha: f64,
    /// 显式弱化事件的比例系数
    pub weaken_alpha: f64,
    /// 单轮处理的到期关系数量
    #[ts(type = "number")]
    pub lifecycle_batch_size: i64,
}

impl MaiBotAMemorixMemoryEvolutionConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/half_life_hours"), self.half_life_hours, Some(0.1), None);
        sink.range(&format!("{path}/freeze_duration_hours"), self.freeze_duration_hours, Some(0.0), None);
        sink.range(&format!("{path}/revive_threshold"), self.revive_threshold, None, Some(1.0));
        sink.range(&format!("{path}/access_reinforcement_alpha"), self.access_reinforcement_alpha, Some(0.0), Some(1.0));
        sink.range(&format!("{path}/access_reinforcement_cooldown_minutes"), self.access_reinforcement_cooldown_minutes, Some(0.0), None);
        sink.range(&format!("{path}/explicit_reinforcement_alpha"), self.explicit_reinforcement_alpha, Some(0.0), Some(1.0));
        sink.range(&format!("{path}/weaken_alpha"), self.weaken_alpha, Some(0.0), Some(1.0));
        sink.range(&format!("{path}/lifecycle_batch_size"), self.lifecycle_batch_size as f64, Some(1.0), None);
    }
}

impl Default for MaiBotAMemorixMemoryEvolutionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            half_life_hours: 24.0,
            prune_threshold: 0.1,
            freeze_duration_hours: 24.0,
            revive_threshold: 0.15,
            access_reinforcement_alpha: 0.05,
            access_reinforcement_cooldown_minutes: 60.0,
            explicit_reinforcement_alpha: 0.5,
            weaken_alpha: 0.5,
            lifecycle_batch_size: 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixAdvancedConfig {
    pub enable_auto_save: bool,
    #[ts(type = "number")]
    pub auto_save_interval_minutes: i64,
    /// 是否启用调试
    pub debug: bool,
}

impl MaiBotAMemorixAdvancedConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotAMemorixAdvancedConfig {
    fn default() -> Self {
        Self {
            enable_auto_save: true,
            auto_save_interval_minutes: 5,
            debug: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixWebImportTimeoutConfig {
    /// Web 导入中单次 LLM 抽取调用的超时时间，0 表示不额外限制
    pub llm_call_seconds: f64,
    pub process_poll_seconds: f64,
    pub process_terminate_seconds: f64,
    pub process_kill_seconds: f64,
    /// LPMM 转换依赖预检的超时时间
    pub convert_preflight_seconds: f64,
}

impl MaiBotAMemorixWebImportTimeoutConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/llm_call_seconds"), self.llm_call_seconds, Some(0.0), None);
        sink.range(&format!("{path}/convert_preflight_seconds"), self.convert_preflight_seconds, Some(0.1), None);
    }
}

impl Default for MaiBotAMemorixWebImportTimeoutConfig {
    fn default() -> Self {
        Self {
            llm_call_seconds: 240.0,
            process_poll_seconds: 1.0,
            process_terminate_seconds: 5.0,
            process_kill_seconds: 3.0,
            convert_preflight_seconds: 20.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixWebImportConfig {
    /// 是否启用导入中心
    pub enabled: bool,
    #[ts(type = "number")]
    pub max_queue_size: i64,
    /// 单任务最大文件数
    #[ts(type = "number")]
    pub max_files_per_task: i64,
    /// 单文件大小上限 MB
    #[ts(type = "number")]
    pub max_file_size_mb: i64,
    /// 粘贴字符数上限
    #[ts(type = "number")]
    pub max_paste_chars: i64,
    #[ts(type = "number")]
    pub default_file_concurrency: i64,
    #[ts(type = "number")]
    pub default_chunk_concurrency: i64,
    /// 默认叙事抽取窗口字符数
    #[ts(type = "number")]
    pub default_narrative_window_size: i64,
    /// 默认叙事窗口重叠字符数
    #[ts(type = "number")]
    pub default_narrative_overlap: i64,
    /// 默认事实分块目标字符数
    #[ts(type = "number")]
    pub default_factual_target_size: i64,
    #[ts(type = "number")]
    pub max_chunk_chars: i64,
    /// 导入中心超时配置
    pub timeout: MaiBotAMemorixWebImportTimeoutConfig,
}

impl MaiBotAMemorixWebImportConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/max_files_per_task"), self.max_files_per_task as f64, Some(1.0), None);
        sink.range(&format!("{path}/max_file_size_mb"), self.max_file_size_mb as f64, Some(1.0), None);
        sink.range(&format!("{path}/max_paste_chars"), self.max_paste_chars as f64, Some(100.0), None);
        sink.range(&format!("{path}/default_narrative_window_size"), self.default_narrative_window_size as f64, Some(200.0), None);
        sink.range(&format!("{path}/default_narrative_overlap"), self.default_narrative_overlap as f64, Some(0.0), None);
        sink.range(&format!("{path}/default_factual_target_size"), self.default_factual_target_size as f64, Some(200.0), None);
        self.timeout.validate(&format!("{path}/timeout"), sink);
    }
}

impl Default for MaiBotAMemorixWebImportConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_queue_size: 20,
            max_files_per_task: 200,
            max_file_size_mb: 20,
            max_paste_chars: 200000,
            default_file_concurrency: 2,
            default_chunk_concurrency: 4,
            default_narrative_window_size: 1600,
            default_narrative_overlap: 400,
            default_factual_target_size: 1200,
            max_chunk_chars: 3200,
            timeout: MaiBotAMemorixWebImportTimeoutConfig { llm_call_seconds: 240.0, process_poll_seconds: 1.0, process_terminate_seconds: 5.0, process_kill_seconds: 3.0, convert_preflight_seconds: 20.0 },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixWebTuningConfig {
    /// 是否启用调优中心
    pub enabled: bool,
    #[ts(type = "number")]
    pub max_queue_size: i64,
    /// 轮询间隔毫秒数
    #[ts(type = "number")]
    pub poll_interval_ms: i64,
    /// 默认调优强度
    #[ts(type = "\"quick\" | \"standard\" | \"deep\"")]
    pub default_intensity: String,
    /// 默认调优目标
    #[ts(type = "\"precision_priority\" | \"balanced\" | \"recall_priority\"")]
    pub default_objective: String,
    /// 默认评估 Top-K
    #[ts(type = "number")]
    pub default_top_k_eval: i64,
    /// 默认样本数
    #[ts(type = "number")]
    pub default_sample_size: i64,
}

impl MaiBotAMemorixWebTuningConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/poll_interval_ms"), self.poll_interval_ms as f64, Some(200.0), None);
        sink.one_of(&format!("{path}/default_intensity"), &self.default_intensity, &["quick", "standard", "deep"]);
        sink.one_of(&format!("{path}/default_objective"), &self.default_objective, &["precision_priority", "balanced", "recall_priority"]);
        sink.range(&format!("{path}/default_top_k_eval"), self.default_top_k_eval as f64, Some(1.0), None);
        sink.range(&format!("{path}/default_sample_size"), self.default_sample_size as f64, Some(1.0), None);
    }
}

impl Default for MaiBotAMemorixWebTuningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_queue_size: 8,
            poll_interval_ms: 1200,
            default_intensity: String::from("standard"),
            default_objective: String::from("precision_priority"),
            default_top_k_eval: 20,
            default_sample_size: 24,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixWebConfig {
    /// 导入中心配置
    pub import_config: MaiBotAMemorixWebImportConfig,
    /// 调优中心配置
    pub tuning: MaiBotAMemorixWebTuningConfig,
}

impl MaiBotAMemorixWebConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        self.import_config.validate(&format!("{path}/import_config"), sink);
        self.tuning.validate(&format!("{path}/tuning"), sink);
    }
}

impl Default for MaiBotAMemorixWebConfig {
    fn default() -> Self {
        Self {
            import_config: MaiBotAMemorixWebImportConfig { enabled: true, max_queue_size: 20, max_files_per_task: 200, max_file_size_mb: 20, max_paste_chars: 200000, default_file_concurrency: 2, default_chunk_concurrency: 4, default_narrative_window_size: 1600, default_narrative_overlap: 400, default_factual_target_size: 1200, max_chunk_chars: 3200, timeout: MaiBotAMemorixWebImportTimeoutConfig { llm_call_seconds: 240.0, process_poll_seconds: 1.0, process_terminate_seconds: 5.0, process_kill_seconds: 3.0, convert_preflight_seconds: 20.0 } },
            tuning: MaiBotAMemorixWebTuningConfig { enabled: true, max_queue_size: 8, poll_interval_ms: 1200, default_intensity: String::from("standard"), default_objective: String::from("precision_priority"), default_top_k_eval: 20, default_sample_size: 24 },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAMemorixConfig {
    /// 长期记忆系统的总开关
    pub plugin: MaiBotAMemorixPluginConfig,
    /// 控制麦麦在聊天中如何使用长期记忆
    pub integration: MaiBotAMemorixIntegrationConfig,
    pub storage: MaiBotAMemorixStorageConfig,
    /// 把记忆内容转换为向量时使用的基础设置
    pub embedding: MaiBotAMemorixEmbeddingConfig,
    /// 检索配置
    pub retrieval: MaiBotAMemorixRetrievalConfig,
    /// 阈值过滤配置
    pub threshold: MaiBotAMemorixThresholdConfig,
    /// 聊天过滤配置
    pub filter: MaiBotAMemorixFilterConfig,
    /// 是否让普通记忆查询在所有聊天流范围内检索
    pub global_memory_sharing_enabled: bool,
    /// 把需要互相参考长期记忆的群聊或私聊放到同一组
    pub shared_memory_groups: Vec<MaiBotChatStreamGroup>,
    /// Episode 配置
    pub episode: MaiBotAMemorixEpisodeConfig,
    /// 人物画像配置
    pub person_profile: MaiBotAMemorixPersonProfileConfig,
    /// 记忆演化配置
    pub memory: MaiBotAMemorixMemoryEvolutionConfig,
    /// 高级运行时配置
    pub advanced: MaiBotAMemorixAdvancedConfig,
    /// Web 运维配置
    pub web: MaiBotAMemorixWebConfig,
}

impl MaiBotAMemorixConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        self.plugin.validate(&format!("{path}/plugin"), sink);
        self.integration.validate(&format!("{path}/integration"), sink);
        self.storage.validate(&format!("{path}/storage"), sink);
        self.embedding.validate(&format!("{path}/embedding"), sink);
        self.retrieval.validate(&format!("{path}/retrieval"), sink);
        self.threshold.validate(&format!("{path}/threshold"), sink);
        self.filter.validate(&format!("{path}/filter"), sink);
        for (i, item) in self.shared_memory_groups.iter().enumerate() {
            item.validate(&format!("{path}/shared_memory_groups/{i}"), sink);
        }
        self.episode.validate(&format!("{path}/episode"), sink);
        self.person_profile.validate(&format!("{path}/person_profile"), sink);
        self.memory.validate(&format!("{path}/memory"), sink);
        self.advanced.validate(&format!("{path}/advanced"), sink);
        self.web.validate(&format!("{path}/web"), sink);
    }
}

impl Default for MaiBotAMemorixConfig {
    fn default() -> Self {
        Self {
            plugin: MaiBotAMemorixPluginConfig { enabled: false },
            integration: MaiBotAMemorixIntegrationConfig { enable_memory_query_tool: true, memory_query_default_limit: 5, enable_person_profile_query_tool: true, enable_person_profile_injection: true, person_profile_injection_max_profiles: 3, heuristic_memory_recall_enabled: false, heuristic_memory_cross_chat_enabled: false, heuristic_memory_recall_window_size: 20, heuristic_memory_recall_limit: 3, heuristic_memory_recall_max_chars: 900, heuristic_memory_recall_min_interval_seconds: 180, heuristic_memory_recall_min_new_messages: 60, heuristic_memory_recall_cache_ttl_seconds: 300, heuristic_memory_group_to_private_enabled: false, heuristic_memory_private_to_group_enabled: false, person_fact_writeback_enabled: true, chat_summary_writeback_enabled: true, chat_summary_writeback_message_threshold: 36, chat_summary_writeback_context_length: 36, fuzzy_modify_enabled: true, fuzzy_modify_auto_execute_enabled: false, fuzzy_modify_confirm_threshold: 0.85, fuzzy_modify_candidate_limit: 20, fuzzy_modify_max_targets: 5, fuzzy_modify_allow_global_scope: false, feedback_correction_enabled: false, feedback_correction_window_hours: 12.0, feedback_correction_check_interval_minutes: 30, feedback_correction_batch_size: 20, feedback_correction_auto_apply_threshold: 0.85, feedback_correction_max_feedback_messages: 30, feedback_correction_prefilter_enabled: true, feedback_correction_paragraph_mark_enabled: true, feedback_correction_paragraph_hard_filter_enabled: true, feedback_correction_profile_refresh_enabled: true, feedback_correction_profile_force_refresh_on_read: true, feedback_correction_episode_rebuild_enabled: true, feedback_correction_episode_query_block_enabled: true, feedback_correction_reconcile_interval_minutes: 5, feedback_correction_reconcile_batch_size: 20 },
            storage: MaiBotAMemorixStorageConfig { data_dir: String::from("data/a-memorix") },
            embedding: MaiBotAMemorixEmbeddingConfig { model_name: String::from("auto"), dimension: 1024, dimension_request_mode: String::from("explicit"), batch_size: 32, max_concurrent: 5, enable_cache: false, runtime_train_threshold: 256, quantization_type: String::from("int8"), fallback: MaiBotAMemorixEmbeddingFallbackConfig { enabled: true, probe_interval_seconds: 180, allow_metadata_only_write: true }, paragraph_vector_backfill: MaiBotAMemorixParagraphVectorBackfillConfig { enabled: true, interval_seconds: 60, batch_size: 64, max_retry: 5 } },
            retrieval: MaiBotAMemorixRetrievalConfig { top_k_paragraphs: 20, top_k_relations: 10, top_k_final: 10, alpha: 0.5, enable_ppr: true, ppr_alpha: 0.85, ppr_timeout_seconds: 1.5, ppr_concurrency_limit: 4, enable_parallel: true, search: MaiBotAMemorixRetrievalSearchConfig { smart_fallback: MaiBotAMemorixSmartFallbackConfig { enabled: true } }, fusion: MaiBotAMemorixFusionRetrievalConfig { method: String::from("weighted_rrf"), rrf_k: 60, vector_weight: 0.7, bm25_weight: 0.3 }, relation_vectorization: MaiBotAMemorixRelationVectorizationConfig { enabled: false, backfill_enabled: false, write_on_import: true }, vector_pools: MaiBotAMemorixVectorPoolsConfig { mode: String::from("dual"), paragraph_top_k: 20, graph_top_k: 40, graph_expand_paragraph_k: 80, relation_expand_per_hit: 5, entity_expand_per_hit: 8, relation_evidence_weight: 1.0, entity_evidence_weight: 0.55, semantic_weight: 0.65, sparse_weight: 0.2, graph_weight: 0.15, relation_intent: MaiBotAMemorixRelationIntentVectorPoolConfig { graph_top_k: 80, semantic_weight: 0.45, sparse_weight: 0.15, graph_weight: 0.4, return_relation_items: false } }, sparse: MaiBotAMemorixSparseRetrievalConfig { enabled: true, backend: String::from("fts5"), mode: String::from("auto"), tokenizer_mode: String::from("jieba"), candidate_k: 80, relation_candidate_k: 60 } },
            threshold: MaiBotAMemorixThresholdConfig { min_threshold: 0.29, max_threshold: 0.95, percentile: 75, min_results: 4 },
            filter: MaiBotAMemorixFilterConfig { enabled: true, mode: String::from("blacklist"), chats: vec![], retrieval: MaiBotAMemorixRetrievalFilterConfig { chat_stream: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] }, chat_summary: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] }, episode: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] } } },
            global_memory_sharing_enabled: false,
            shared_memory_groups: vec![],
            episode: MaiBotAMemorixEpisodeConfig { enabled: true, generation_enabled: true, source_poll_interval_seconds: 1.0, source_batch_size: 20, source_max_retry: 3, source_lease_seconds: 1800.0, source_max_wait_seconds: 60.0, max_paragraphs_per_call: 20, max_chars_per_call: 6000, source_time_window_hours: 24.0, segmentation_model: String::from("auto"), disabled_source_types: vec![String::from("person_fact")] },
            person_profile: MaiBotAMemorixPersonProfileConfig { enabled: true, refresh_interval_minutes: 30, active_window_hours: 72.0, max_refresh_per_cycle: 50, refresh_debounce_seconds: 120, refresh_queue_interval_seconds: 60, refresh_queue_batch_size: 10, refresh_retry_backoff_seconds: 300, max_retry: 3, top_k_evidence: 12, evidence_classification_max_tokens: 1200, evidence_classification_temperature: 0.1 },
            memory: MaiBotAMemorixMemoryEvolutionConfig { enabled: true, half_life_hours: 24.0, prune_threshold: 0.1, freeze_duration_hours: 24.0, revive_threshold: 0.15, access_reinforcement_alpha: 0.05, access_reinforcement_cooldown_minutes: 60.0, explicit_reinforcement_alpha: 0.5, weaken_alpha: 0.5, lifecycle_batch_size: 1000 },
            advanced: MaiBotAMemorixAdvancedConfig { enable_auto_save: true, auto_save_interval_minutes: 5, debug: false },
            web: MaiBotAMemorixWebConfig { import_config: MaiBotAMemorixWebImportConfig { enabled: true, max_queue_size: 20, max_files_per_task: 200, max_file_size_mb: 20, max_paste_chars: 200000, default_file_concurrency: 2, default_chunk_concurrency: 4, default_narrative_window_size: 1600, default_narrative_overlap: 400, default_factual_target_size: 1200, max_chunk_chars: 3200, timeout: MaiBotAMemorixWebImportTimeoutConfig { llm_call_seconds: 240.0, process_poll_seconds: 1.0, process_terminate_seconds: 5.0, process_kill_seconds: 3.0, convert_preflight_seconds: 20.0 } }, tuning: MaiBotAMemorixWebTuningConfig { enabled: true, max_queue_size: 8, poll_interval_ms: 1200, default_intensity: String::from("standard"), default_objective: String::from("precision_priority"), default_top_k_eval: 20, default_sample_size: 24 } },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotMessageReceiveConfig {
    /// 单条消息图片数不超过这个值时才识图，避免图片太多拖慢处理。
    #[ts(type = "number")]
    pub image_parse_threshold: i64,
    /// 包含这些词的消息会被过滤，不进入麦麦处理。
    pub ban_words: Vec<String>,
    /// 用正则过滤消息；适合更复杂的过滤规则。
    pub ban_msgs_regex: Vec<String>,
}

impl MaiBotMessageReceiveConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotMessageReceiveConfig {
    fn default() -> Self {
        Self {
            image_parse_threshold: 5,
            ban_words: vec![],
            ban_msgs_regex: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotVoiceConfig {
    /// 开启后麦麦可以把语音消息识别成文字再处理。
    pub enable_asr: bool,
}

impl MaiBotVoiceConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotVoiceConfig {
    fn default() -> Self {
        Self {
            enable_asr: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotEmojiCacheCleanupConfig {
    /// 开启后会自动删除长期未注册、未使用的表情包缓存。
    pub enabled: bool,
    /// 每隔多少小时检查一次旧表情包缓存。
    pub check_interval_hours: f64,
    /// 未注册表情包文件多久没被使用后可以删除；已注册表情包永远不会由该任务删除。
    #[ts(type = "number")]
    pub emoji_file_retention_days: i64,
    /// 未注册表情包文件删掉后，描述缓存记录还能保留多久。
    #[ts(type = "number")]
    pub no_file_record_retention_days: i64,
}

impl MaiBotEmojiCacheCleanupConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/check_interval_hours"), self.check_interval_hours, Some(0.016666666666666666), None);
        sink.range(&format!("{path}/emoji_file_retention_days"), self.emoji_file_retention_days as f64, Some(1.0), None);
        sink.range(&format!("{path}/no_file_record_retention_days"), self.no_file_record_retention_days as f64, Some(1.0), None);
    }
}

impl Default for MaiBotEmojiCacheCleanupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_hours: 6.0,
            emoji_file_retention_days: 30,
            no_file_record_retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotEmojiConfig {
    /// 每次从多少个候选表情里挑一个发送；不是一次发送这么多。
    #[ts(type = "number")]
    pub emoji_send_num: i64,
    /// 最多保存多少个可用表情。
    #[ts(type = "number")]
    pub max_reg_num: i64,
    /// 表情满了以后是否用新表情替换旧表情。
    pub do_replace: bool,
    /// 每隔多少分钟检查一次表情库状态。
    #[ts(type = "number")]
    pub check_interval: i64,
    /// 是否从聊天中自动收集别人发的表情。
    pub steal_emoji: bool,
    /// 收集表情时允许的最大文件大小；0 表示不限。
    pub max_emoji_size_mb: f64,
    /// 开启后只保存内容合适的表情。
    pub content_filtration: bool,
    /// 定期清理未注册表情包缓存，减少磁盘占用；已注册表情包不会被清理。
    pub cache_cleanup: MaiBotEmojiCacheCleanupConfig,
}

impl MaiBotEmojiConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/emoji_send_num"), self.emoji_send_num as f64, Some(1.0), Some(64.0));
        sink.range(&format!("{path}/max_emoji_size_mb"), self.max_emoji_size_mb, Some(0.0), None);
        self.cache_cleanup.validate(&format!("{path}/cache_cleanup"), sink);
    }
}

impl Default for MaiBotEmojiConfig {
    fn default() -> Self {
        Self {
            emoji_send_num: 25,
            max_reg_num: 64,
            do_replace: true,
            check_interval: 10,
            steal_emoji: true,
            max_emoji_size_mb: 5.0,
            content_filtration: false,
            cache_cleanup: MaiBotEmojiCacheCleanupConfig { enabled: true, check_interval_hours: 6.0, emoji_file_retention_days: 30, no_file_record_retention_days: 30 },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotKeywordRuleConfig {
    /// 要匹配的关键词；命中任意一个即可触发。
    pub keywords: Vec<String>,
    /// 要匹配的正则表达式；适合复杂文本规则。
    pub regex: Vec<String>,
    /// 命中后给麦麦看的提示内容，不会直接当作消息发送。
    pub reaction: String,
}

impl MaiBotKeywordRuleConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotKeywordRuleConfig {
    fn default() -> Self {
        Self {
            keywords: vec![],
            regex: vec![],
            reaction: String::from(""),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotKeywordReactionConfig {
    /// 命中关键词后，给麦麦追加一段固定反应提示。
    pub keyword_rules: Vec<MaiBotKeywordRuleConfig>,
    /// 命中正则规则后，给麦麦追加一段固定反应提示。
    pub regex_rules: Vec<MaiBotKeywordRuleConfig>,
}

impl MaiBotKeywordReactionConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        for (i, item) in self.keyword_rules.iter().enumerate() {
            item.validate(&format!("{path}/keyword_rules/{i}"), sink);
        }
        for (i, item) in self.regex_rules.iter().enumerate() {
            item.validate(&format!("{path}/regex_rules/{i}"), sink);
        }
    }
}

impl Default for MaiBotKeywordReactionConfig {
    fn default() -> Self {
        Self {
            keyword_rules: vec![],
            regex_rules: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotResponsePostProcessConfig {
    /// 开启后会对回复做错别字、分段等后处理。
    pub enable_response_post_process: bool,
    /// 模拟打字等待时间；0 最快，1 默认，2 更慢。
    pub typing_speed: f64,
}

impl MaiBotResponsePostProcessConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/typing_speed"), self.typing_speed, Some(0.0), Some(2.0));
    }
}

impl Default for MaiBotResponsePostProcessConfig {
    fn default() -> Self {
        Self {
            enable_response_post_process: true,
            typing_speed: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotChineseTypoConfig {
    /// 让麦麦偶尔打错字，更像真人聊天。
    pub enable: bool,
    /// 纠正错别字时，是否引用上一条包含错别字的消息。
    pub enable_correction_quote: bool,
    /// 生成纠正消息时，引用上一条错别字消息的概率。
    pub correction_quote_probability: f64,
    /// 单个字被替换成错字的概率。
    pub error_rate: f64,
    /// 只对常见程度达到该值的字尝试制造错字。
    #[ts(type = "number")]
    pub min_freq: i64,
    /// 按相近声调制造错字的概率。
    pub tone_error_rate: f64,
    /// 整词被替换成错词的概率。
    pub word_replace_rate: f64,
}

impl MaiBotChineseTypoConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/correction_quote_probability"), self.correction_quote_probability, Some(0.0), Some(1.0));
        sink.range(&format!("{path}/error_rate"), self.error_rate, Some(0.0), Some(1.0));
        sink.range(&format!("{path}/tone_error_rate"), self.tone_error_rate, Some(0.0), Some(1.0));
        sink.range(&format!("{path}/word_replace_rate"), self.word_replace_rate, Some(0.0), Some(1.0));
    }
}

impl Default for MaiBotChineseTypoConfig {
    fn default() -> Self {
        Self {
            enable: true,
            enable_correction_quote: true,
            correction_quote_probability: 1.0,
            error_rate: 0.01,
            min_freq: 9,
            tone_error_rate: 0.1,
            word_replace_rate: 0.006,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotResponseSplitterConfig {
    /// 把过长回复拆成多条发送。
    pub enable: bool,
    /// 单条回复允许的最大长度。
    #[ts(type = "number")]
    pub max_length: i64,
    /// 单条回复最多包含多少个句子。
    #[ts(type = "number")]
    pub max_sentence_num: i64,
    /// 一次回复最多拆成几条消息。
    #[ts(type = "number")]
    pub max_split_num: i64,
    /// 尽量避免把颜文字从中间拆开。
    pub enable_kaomoji_protection: bool,
    /// 句子太多时是否直接保留完整回复，不再强行截断。
    pub enable_overflow_return_all: bool,
}

impl MaiBotResponseSplitterConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/max_split_num"), self.max_split_num as f64, Some(1.0), None);
    }
}

impl Default for MaiBotResponseSplitterConfig {
    fn default() -> Self {
        Self {
            enable: true,
            max_length: 512,
            max_sentence_num: 8,
            max_split_num: 3,
            enable_kaomoji_protection: false,
            enable_overflow_return_all: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotTelemetryConfig {
    /// 是否发送匿名运行统计；关闭不影响正常使用。
    pub enable: bool,
}

impl MaiBotTelemetryConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotTelemetryConfig {
    fn default() -> Self {
        Self {
            enable: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotLogConfig {
    /// 日志时间的显示格式。
    pub date_style: String,
    /// 日志等级的显示样式，只影响日志外观。
    #[ts(type = "\"lite\" | \"compact\" | \"full\"")]
    pub log_level_style: String,
    /// 控制台日志颜色范围。
    #[ts(type = "\"none\" | \"title\" | \"full\"")]
    pub color_text: String,
    /// 全局最低日志等级；DEBUG 最详细，ERROR 最安静。
    #[ts(type = "\"DEBUG\" | \"INFO\" | \"WARNING\" | \"ERROR\" | \"CRITICAL\"")]
    pub log_level: String,
    /// 控制台输出的最低日志等级。
    #[ts(type = "\"DEBUG\" | \"INFO\" | \"WARNING\" | \"ERROR\" | \"CRITICAL\"")]
    pub console_log_level: String,
    /// 写入日志文件的最低日志等级。
    #[ts(type = "\"DEBUG\" | \"INFO\" | \"WARNING\" | \"ERROR\" | \"CRITICAL\"")]
    pub file_log_level: String,
    /// 单个日志文件超过这个大小后会轮转。
    #[ts(type = "number")]
    pub log_file_max_bytes: i64,
    /// 最多保留多少个主日志文件。
    #[ts(type = "number")]
    pub max_log_files: i64,
    /// 主日志文件超过多少天后清理。
    #[ts(type = "number")]
    pub log_cleanup_days: i64,
    /// 失败模型请求快照最多保留多少份。
    #[ts(type = "number")]
    pub llm_request_snapshot_limit: i64,
    /// 每个聊天最多保留多少组 Prompt 预览。
    #[ts(type = "number")]
    pub maisaka_prompt_preview_limit: i64,
    /// 每个聊天最多保留多少条回复效果记录。
    #[ts(type = "number")]
    pub maisaka_reply_effect_limit: i64,
    /// 完全不显示日志的第三方库名称列表。
    pub suppress_libraries: Vec<String>,
    /// 单独设置某些第三方库的日志等级。
    pub library_log_levels: std::collections::BTreeMap<String, String>,
}

impl MaiBotLogConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/log_level_style"), &self.log_level_style, &["lite", "compact", "full"]);
        sink.one_of(&format!("{path}/color_text"), &self.color_text, &["none", "title", "full"]);
        sink.one_of(&format!("{path}/log_level"), &self.log_level, &["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"]);
        sink.one_of(&format!("{path}/console_log_level"), &self.console_log_level, &["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"]);
        sink.one_of(&format!("{path}/file_log_level"), &self.file_log_level, &["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"]);
    }
}

impl Default for MaiBotLogConfig {
    fn default() -> Self {
        Self {
            date_style: String::from("m-d H:i:s"),
            log_level_style: String::from("lite"),
            color_text: String::from("full"),
            log_level: String::from("INFO"),
            console_log_level: String::from("INFO"),
            file_log_level: String::from("DEBUG"),
            log_file_max_bytes: 5242880,
            max_log_files: 30,
            log_cleanup_days: 30,
            llm_request_snapshot_limit: 128,
            maisaka_prompt_preview_limit: 256,
            maisaka_reply_effect_limit: 256,
            suppress_libraries: vec![String::from("faiss"), String::from("httpx"), String::from("urllib3"), String::from("asyncio"), String::from("websockets"), String::from("httpcore"), String::from("requests"), String::from("sqlalchemy"), String::from("openai"), String::from("uvicorn"), String::from("jieba")],
            library_log_levels: std::collections::BTreeMap::from([(String::from("aiohttp"), String::from("WARNING")), (String::from("PIL"), String::from("WARNING"))]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotDebugConfig {
    /// 在交互式终端中启用本地消息和指令输入。
    pub enable_console_input: bool,
    /// 在日志或界面中显示麦麦的思考过程。
    pub show_maisaka_thinking: bool,
    /// 允许使用 /clear 清空当前聊天流的 Maisaka 短期历史上下文。
    pub enable_clear_context_command: bool,
    /// 记录回复效果评分，方便观察回复质量。
    pub enable_reply_effect_tracking: bool,
    /// Prompt 预览里保留图片 base64，便于复现但会占空间。
    pub keep_prompt_preview_json_base64: bool,
    /// 保存工具返回的结构化内容，便于调试但会增加数据库体积。
    pub record_tool_structured_content: bool,
    /// 记录模型 prompt cache 统计，用于性能调试。
    pub enable_llm_cache_stats: bool,
}

impl MaiBotDebugConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotDebugConfig {
    fn default() -> Self {
        Self {
            enable_console_input: false,
            show_maisaka_thinking: true,
            enable_clear_context_command: false,
            enable_reply_effect_tracking: false,
            keep_prompt_preview_json_base64: false,
            record_tool_structured_content: false,
            enable_llm_cache_stats: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotMaimMessageConfig {
    /// 旧版 WebSocket 服务监听地址；不清楚就保持默认。
    pub ws_server_host: String,
    /// 旧版 WebSocket 服务端口。
    #[ts(type = "number")]
    pub ws_server_port: i64,
    /// 旧版 API 的认证令牌；为空表示不验证。
    pub auth_token: Vec<String>,
    /// 是否开启新版 API Server，供外部程序调用麦麦。
    pub enable_api_server: bool,
    /// 新版 API Server 监听地址；0.0.0.0 表示允许外部访问。
    pub api_server_host: String,
    /// 新版 API Server 监听端口。
    #[ts(type = "number")]
    pub api_server_port: i64,
    /// 新版 API Server 是否使用加密 WebSocket。
    pub api_server_use_wss: bool,
    /// WSS 使用的证书文件路径。
    pub api_server_cert_file: String,
    /// WSS 使用的私钥文件路径。
    pub api_server_key_file: String,
    /// 允许访问新版 API 的 Key 列表；为空表示不限制。
    pub api_server_allowed_api_keys: Vec<String>,
}

impl MaiBotMaimMessageConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotMaimMessageConfig {
    fn default() -> Self {
        Self {
            ws_server_host: String::from("127.0.0.1"),
            ws_server_port: 8000,
            auth_token: vec![],
            enable_api_server: false,
            api_server_host: String::from("0.0.0.0"),
            api_server_port: 8090,
            api_server_use_wss: false,
            api_server_cert_file: String::from(""),
            api_server_key_file: String::from(""),
            api_server_allowed_api_keys: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotWebUIConfig {
    /// 是否启动 WebUI 管理界面。
    pub enabled: bool,
    /// WebUI 监听地址列表；可同时绑定 IPv4 和 IPv6，例如 ["0.0.0.0", "::"]。
    pub host: Vec<String>,
    /// WebUI 访问端口。
    #[ts(type = "number")]
    pub port: i64,
    /// WebUI 运行模式；普通使用保持 production。
    #[ts(type = "\"development\" | \"production\"")]
    pub mode: String,
    /// 界面风格编号；0 为旧风格，1 为未来复古风格。
    #[ts(type = "number")]
    pub webui_style: i64,
    /// 防爬虫策略；basic 只记录，strict/loose 会拦截更多请求。
    #[ts(type = "\"false\" | \"strict\" | \"loose\" | \"basic\"")]
    pub anti_crawler_mode: String,
    /// 允许访问 WebUI 的 IP，多个用逗号分隔。
    pub allowed_ips: String,
    /// 可信反向代理 IP；只有这些代理传来的真实 IP 会被信任。
    pub trusted_proxies: String,
    /// 是否信任 X-Forwarded-For 里的真实访客 IP。
    pub trust_xff: bool,
    /// 只在 HTTPS 下发送登录 Cookie；没有 HTTPS 时不要开启。
    pub secure_cookie: bool,
    /// 限制 WebUI 访问外部 URL，降低访问内网地址的风险。
    pub enforce_public_outbound_url: bool,
    /// 知识图谱里是否加载段落全文；更完整但更占内存。
    pub enable_paragraph_content: bool,
}

impl MaiBotWebUIConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/mode"), &self.mode, &["development", "production"]);
        sink.range(&format!("{path}/webui_style"), self.webui_style as f64, Some(0.0), Some(1.0));
        sink.one_of(&format!("{path}/anti_crawler_mode"), &self.anti_crawler_mode, &["false", "strict", "loose", "basic"]);
    }
}

impl Default for MaiBotWebUIConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: vec![String::from("127.0.0.1"), String::from("::1")],
            port: 8001,
            mode: String::from("production"),
            webui_style: 1,
            anti_crawler_mode: String::from("basic"),
            allowed_ips: String::from("127.0.0.1"),
            trusted_proxies: String::from(""),
            trust_xff: false,
            secure_cookie: false,
            enforce_public_outbound_url: true,
            enable_paragraph_content: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotDatabaseConfig {
    /// 是否保存语音等二进制原文件；更占空间，但方便以后重新识别。
    pub save_binary_data: bool,
}

impl MaiBotDatabaseConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotDatabaseConfig {
    fn default() -> Self {
        Self {
            save_binary_data: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotMCPRootItemConfig {
    /// 是否启用这个 Root。
    pub enabled: bool,
    /// Root 的 URI，文件夹一般写 file:/// 开头的路径。
    pub uri: String,
    /// 这个 Root 在 MCP 里的显示名称。
    pub name: String,
}

impl MaiBotMCPRootItemConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotMCPRootItemConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            uri: String::from(""),
            name: String::from(""),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotMCPRootsConfig {
    /// 是否向 MCP 服务器暴露 Roots 能力。
    pub enable: bool,
    /// 允许 MCP 服务器看到的目录或资源列表。
    pub items: Vec<MaiBotMCPRootItemConfig>,
}

impl MaiBotMCPRootsConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        for (i, item) in self.items.iter().enumerate() {
            item.validate(&format!("{path}/items/{i}"), sink);
        }
    }
}

impl Default for MaiBotMCPRootsConfig {
    fn default() -> Self {
        Self {
            enable: false,
            items: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotMCPSamplingConfig {
    /// 是否声明支持 MCP Sampling。
    pub enable: bool,
    /// MCP Sampling 调用模型时使用的任务名。
    pub task_name: String,
    /// 是否允许 Sampling 请求带上下文。
    pub include_context_support: bool,
    /// Sampling 过程中是否允许继续使用工具。
    pub tool_support: bool,
}

impl MaiBotMCPSamplingConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotMCPSamplingConfig {
    fn default() -> Self {
        Self {
            enable: false,
            task_name: String::from("planner"),
            include_context_support: false,
            tool_support: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotMCPElicitationConfig {
    /// 是否声明支持 MCP Elicitation。
    pub enable: bool,
    /// 是否允许 MCP 服务器请求填写表单。
    pub allow_form: bool,
    /// 是否允许 MCP 服务器请求打开 URL。
    pub allow_url: bool,
}

impl MaiBotMCPElicitationConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotMCPElicitationConfig {
    fn default() -> Self {
        Self {
            enable: false,
            allow_form: true,
            allow_url: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotMCPClientConfig {
    /// 对 MCP 服务器展示的客户端名称。
    pub client_name: String,
    /// 对 MCP 服务器展示的客户端版本。
    pub client_version: String,
    /// 是否向 MCP 服务器提供可访问的文件根目录。
    pub roots: MaiBotMCPRootsConfig,
    /// 是否允许 MCP 服务器请求麦麦调用模型。
    pub sampling: MaiBotMCPSamplingConfig,
    /// 是否允许 MCP 服务器向麦麦请求补充信息。
    pub elicitation: MaiBotMCPElicitationConfig,
}

impl MaiBotMCPClientConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        self.roots.validate(&format!("{path}/roots"), sink);
        self.sampling.validate(&format!("{path}/sampling"), sink);
        self.elicitation.validate(&format!("{path}/elicitation"), sink);
    }
}

impl Default for MaiBotMCPClientConfig {
    fn default() -> Self {
        Self {
            client_name: String::from("MaiBot"),
            client_version: String::from("1.0.0"),
            roots: MaiBotMCPRootsConfig { enable: false, items: vec![] },
            sampling: MaiBotMCPSamplingConfig { enable: false, task_name: String::from("planner"), include_context_support: false, tool_support: false },
            elicitation: MaiBotMCPElicitationConfig { enable: false, allow_form: true, allow_url: false },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotMCPAuthorizationConfig {
    /// MCP HTTP 认证方式；none 表示不认证。
    #[ts(type = "\"none\" | \"bearer\"")]
    pub mode: String,
    /// Bearer 认证令牌，只在 mode 为 bearer 时使用。
    pub bearer_token: String,
}

impl MaiBotMCPAuthorizationConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/mode"), &self.mode, &["none", "bearer"]);
    }
}

impl Default for MaiBotMCPAuthorizationConfig {
    fn default() -> Self {
        Self {
            mode: String::from("none"),
            bearer_token: String::from(""),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotMCPServerItemConfig {
    /// MCP 服务器名称，必须唯一。
    pub name: String,
    /// 是否启用这个 MCP 服务器。
    pub enabled: bool,
    /// 连接方式；本地命令通常用 stdio，远程服务用 HTTP/SSE。
    #[ts(type = "\"stdio\" | \"streamable_http\" | \"sse\"")]
    pub transport: String,
    /// stdio 模式下启动服务器的命令。
    pub command: String,
    /// stdio 模式下传给命令的参数。
    pub args: Vec<String>,
    /// stdio 模式下额外传入的环境变量。
    pub env: std::collections::BTreeMap<String, String>,
    /// HTTP 或 SSE 模式下的服务器地址。
    pub url: String,
    /// HTTP/SSE 请求时附加的请求头。
    pub headers: std::collections::BTreeMap<String, String>,
    /// HTTP 请求多久没响应就算超时。
    pub http_timeout_seconds: f64,
    /// 连接建立后，等服务器消息的最长时间。
    pub read_timeout_seconds: f64,
    /// HTTP/SSE 连接的认证设置。
    pub authorization: MaiBotMCPAuthorizationConfig,
}

impl MaiBotMCPServerItemConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.one_of(&format!("{path}/transport"), &self.transport, &["stdio", "streamable_http", "sse"]);
        self.authorization.validate(&format!("{path}/authorization"), sink);
    }
}

impl Default for MaiBotMCPServerItemConfig {
    fn default() -> Self {
        Self {
            name: String::from(""),
            enabled: true,
            transport: String::from("stdio"),
            command: String::from(""),
            args: vec![],
            env: std::collections::BTreeMap::new(),
            url: String::from(""),
            headers: std::collections::BTreeMap::new(),
            http_timeout_seconds: 30.0,
            read_timeout_seconds: 300.0,
            authorization: MaiBotMCPAuthorizationConfig { mode: String::from("none"), bearer_token: String::from("") },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotMCPConfig {
    /// 是否启用 MCP 工具接入能力。
    pub enable: bool,
    /// 麦麦作为 MCP 客户端时声明的能力。
    pub client: MaiBotMCPClientConfig,
    /// 要连接的 MCP 服务器列表。
    pub servers: Vec<MaiBotMCPServerItemConfig>,
}

impl MaiBotMCPConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        self.client.validate(&format!("{path}/client"), sink);
        for (i, item) in self.servers.iter().enumerate() {
            item.validate(&format!("{path}/servers/{i}"), sink);
        }
    }
}

impl Default for MaiBotMCPConfig {
    fn default() -> Self {
        Self {
            enable: true,
            client: MaiBotMCPClientConfig { client_name: String::from("MaiBot"), client_version: String::from("1.0.0"), roots: MaiBotMCPRootsConfig { enable: false, items: vec![] }, sampling: MaiBotMCPSamplingConfig { enable: false, task_name: String::from("planner"), include_context_support: false, tool_support: false }, elicitation: MaiBotMCPElicitationConfig { enable: false, allow_form: true, allow_url: false } },
            servers: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotCommandPermissionConfig {
    /// 允许执行命令的用户，格式如 qq:123456789。
    pub allow_users: Vec<String>,
    /// 允许执行命令的真实聊天流 ID。
    pub allow_chats: Vec<String>,
}

impl MaiBotCommandPermissionConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        let _ = (path, sink);
    }
}

impl Default for MaiBotCommandPermissionConfig {
    fn default() -> Self {
        Self {
            allow_users: vec![],
            allow_chats: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotPluginConfig {
    /// 允许用聊天命令管理插件的用户，格式如 qq:123456789。
    pub permission: Vec<String>,
    /// 受保护命令按用户和真实聊天流配置的额外放行规则。
    pub command_permissions: std::collections::BTreeMap<String, MaiBotCommandPermissionConfig>,
}

impl MaiBotPluginConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        for (k, item) in &self.command_permissions {
            item.validate(&format!("{path}/command_permissions/{k}"), sink);
        }
    }
}

impl Default for MaiBotPluginConfig {
    fn default() -> Self {
        Self {
            permission: vec![],
            command_permissions: std::collections::BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotPluginRuntimeRenderConfig {
    /// 是否允许插件使用浏览器渲染能力。
    pub enabled: bool,
    /// 已有 Chrome/Chromium 的调试地址；留空则自动启动。
    pub browser_ws_endpoint: String,
    /// 浏览器程序路径；留空自动查找。
    pub executable_path: String,
    /// 自动下载浏览器时保存的位置。
    pub browser_install_root: String,
    /// 是否隐藏浏览器窗口运行。
    pub headless: bool,
    /// 启动浏览器时附加的命令参数。
    pub launch_args: Vec<String>,
    /// 同时最多运行多少个渲染任务。
    #[ts(type = "number")]
    pub concurrency_limit: i64,
    /// 浏览器启动或连接的最长等待时间。
    pub startup_timeout_sec: f64,
    /// 单次渲染任务的最长等待时间。
    pub render_timeout_sec: f64,
    /// 找不到浏览器时是否自动下载 Chromium。
    pub auto_download_chromium: bool,
    /// 下载 Chromium 时的连接超时时间。
    pub download_connection_timeout_sec: f64,
    /// 渲染多少次后重启浏览器；0 表示不自动重启。
    #[ts(type = "number")]
    pub restart_after_render_count: i64,
}

impl MaiBotPluginRuntimeRenderConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/concurrency_limit"), self.concurrency_limit as f64, Some(1.0), None);
        sink.range(&format!("{path}/restart_after_render_count"), self.restart_after_render_count as f64, Some(0.0), None);
    }
}

impl Default for MaiBotPluginRuntimeRenderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            browser_ws_endpoint: String::from(""),
            executable_path: String::from(""),
            browser_install_root: String::from("data/playwright-browsers"),
            headless: true,
            launch_args: vec![String::from("--disable-gpu"), String::from("--disable-dev-shm-usage"), String::from("--disable-setuid-sandbox"), String::from("--no-sandbox"), String::from("--no-zygote")],
            concurrency_limit: 2,
            startup_timeout_sec: 20.0,
            render_timeout_sec: 15.0,
            auto_download_chromium: true,
            download_connection_timeout_sec: 120.0,
            restart_after_render_count: 200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotPluginRuntimeConfig {
    /// 是否启用新版插件运行时。
    pub enabled: bool,
    /// 每隔多少秒检查一次插件运行状态。
    pub health_check_interval_sec: f64,
    /// 插件 Runner 崩溃后最多自动重启几次。
    #[ts(type = "number")]
    pub max_restart_attempts: i64,
    /// 等待插件 Runner 启动完成的最长时间。
    pub runner_spawn_timeout_sec: f64,
    /// 单个阻塞 Hook 最多允许运行多久。
    pub hook_blocking_timeout_sec: f64,
    /// 自定义插件通信 Socket 路径；留空自动生成。
    pub ipc_socket_path: String,
    /// 插件需要网页截图或渲染时使用的浏览器配置。
    pub render: MaiBotPluginRuntimeRenderConfig,
}

impl MaiBotPluginRuntimeConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        self.render.validate(&format!("{path}/render"), sink);
    }
}

impl Default for MaiBotPluginRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            health_check_interval_sec: 30.0,
            max_restart_attempts: 3,
            runner_spawn_timeout_sec: 30.0,
            hook_blocking_timeout_sec: 60.0,
            ipc_socket_path: String::from(""),
            render: MaiBotPluginRuntimeRenderConfig { enabled: true, browser_ws_endpoint: String::from(""), executable_path: String::from(""), browser_install_root: String::from("data/playwright-browsers"), headless: true, launch_args: vec![String::from("--disable-gpu"), String::from("--disable-dev-shm-usage"), String::from("--disable-setuid-sandbox"), String::from("--no-sandbox"), String::from("--no-zygote")], concurrency_limit: 2, startup_timeout_sec: 20.0, render_timeout_sec: 15.0, auto_download_chromium: true, download_connection_timeout_sec: 120.0, restart_after_render_count: 200 },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotBotConfigFile {
    /// 机器人配置类
    pub bot: MaiBotBotConfig,
    /// 人格配置类
    pub personality: MaiBotPersonalityConfig,
    /// 聊天配置类
    pub chat: MaiBotChatConfig,
    /// 实验性功能配置类
    pub experimental: MaiBotExperimentalConfig,
    /// 视觉配置类
    pub visual: MaiBotVisualConfig,
    /// 表达配置类
    pub expression: MaiBotExpressionConfig,
    /// 黑话配置类
    pub jargon: MaiBotJargonConfig,
    /// A_Memorix 长期记忆子系统配置
    pub a_memorix: MaiBotAMemorixConfig,
    /// 消息接收配置类
    pub message_receive: MaiBotMessageReceiveConfig,
    /// 语音配置类
    pub voice: MaiBotVoiceConfig,
    /// 表情包配置类
    pub emoji: MaiBotEmojiConfig,
    /// 关键词反应配置类
    pub keyword_reaction: MaiBotKeywordReactionConfig,
    /// 回复后处理配置类
    pub response_post_process: MaiBotResponsePostProcessConfig,
    /// 中文错别字生成器配置类
    pub chinese_typo: MaiBotChineseTypoConfig,
    /// 回复分割器配置类
    pub response_splitter: MaiBotResponseSplitterConfig,
    /// 遥测配置类
    pub telemetry: MaiBotTelemetryConfig,
    /// 日志配置类
    pub log: MaiBotLogConfig,
    /// 调试配置类
    pub debug: MaiBotDebugConfig,
    /// maim_message配置类
    pub maim_message: MaiBotMaimMessageConfig,
    /// WebUI配置类
    pub webui: MaiBotWebUIConfig,
    /// 数据库配置类
    pub database: MaiBotDatabaseConfig,
    /// MCP 配置类
    pub mcp: MaiBotMCPConfig,
    /// 插件管理配置类
    pub plugin: MaiBotPluginConfig,
    /// 插件运行时配置类
    pub plugin_runtime: MaiBotPluginRuntimeConfig,
}

impl MaiBotBotConfigFile {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        self.bot.validate(&format!("{path}/bot"), sink);
        self.personality.validate(&format!("{path}/personality"), sink);
        self.chat.validate(&format!("{path}/chat"), sink);
        self.experimental.validate(&format!("{path}/experimental"), sink);
        self.visual.validate(&format!("{path}/visual"), sink);
        self.expression.validate(&format!("{path}/expression"), sink);
        self.jargon.validate(&format!("{path}/jargon"), sink);
        self.a_memorix.validate(&format!("{path}/a_memorix"), sink);
        self.message_receive.validate(&format!("{path}/message_receive"), sink);
        self.voice.validate(&format!("{path}/voice"), sink);
        self.emoji.validate(&format!("{path}/emoji"), sink);
        self.keyword_reaction.validate(&format!("{path}/keyword_reaction"), sink);
        self.response_post_process.validate(&format!("{path}/response_post_process"), sink);
        self.chinese_typo.validate(&format!("{path}/chinese_typo"), sink);
        self.response_splitter.validate(&format!("{path}/response_splitter"), sink);
        self.telemetry.validate(&format!("{path}/telemetry"), sink);
        self.log.validate(&format!("{path}/log"), sink);
        self.debug.validate(&format!("{path}/debug"), sink);
        self.maim_message.validate(&format!("{path}/maim_message"), sink);
        self.webui.validate(&format!("{path}/webui"), sink);
        self.database.validate(&format!("{path}/database"), sink);
        self.mcp.validate(&format!("{path}/mcp"), sink);
        self.plugin.validate(&format!("{path}/plugin"), sink);
        self.plugin_runtime.validate(&format!("{path}/plugin_runtime"), sink);
    }
}

impl Default for MaiBotBotConfigFile {
    fn default() -> Self {
        Self {
            bot: MaiBotBotConfig { platform: String::from(""), qq_account: String::from(""), platforms: vec![], nickname: String::from("麦麦"), alias_names: vec![] },
            personality: MaiBotPersonalityConfig { personality: String::from("是一个大二女大学生，现在正在上网和群友聊天。善于用人类的角度思考问题，聊天偏日常。"), behavior_style: String::from("是大二女大学生，现在正在上网和群友聊天。善于用人类的角度思考问题，聊天偏日常。不会没话题硬找话题，"), reply_style: String::from("你的风格平淡简短，可以参考贴吧的回复风格。不滥用比喻或者生硬句子。视情况省略主语或者进行倒装，风格较为随意。"), multiple_reply_style: vec![String::from("你的风格平淡但不失讽刺，很简短,很白话。可以参考贴吧，微博的回复风格。"), String::from("用1-2个字进行回复"), String::from("用1-2个符号进行回复"), String::from("言辭凝練古雅，穿插《論語》經句卻不晦澀，以文言短句為基，輔以淺白語意，持長者溫和風範，全用繁體字表達，具先秦儒者談吐韻致。"), String::from("带点翻译腔，但不要太长")], multiple_probability: 0.0 },
            chat: MaiBotChatConfig { max_context_size: 40, max_private_context_size: 60, enable_context_optimization: true, mid_term_memory: true, mid_term_memory_lenth: 10, reply_timing: MaiBotChatReplyTimingConfig { talk_value: 1.0, private_talk_value: 1.0, mentioned_bot_reply: false, inevitable_at_reply: true, reply_trigger_mode: String::from("frequency"), planner_interrupt_max_consecutive_count: 0, max_consecutive_wait_count: 3, no_action_backoff_base_seconds: 15.0, no_action_backoff_cap_seconds: 300.0, no_action_backoff_start_count: 2, no_action_backoff_bypass_pending_count: 6, enable_talk_value_rules: false, talk_value_rules: vec![MaiBotTalkRulesItem { platform: String::from(""), item_id: String::from(""), rule_type: String::from("group"), time: String::from("00:00-08:59"), value: 0.8 }, MaiBotTalkRulesItem { platform: String::from(""), item_id: String::from(""), rule_type: String::from("group"), time: String::from("09:00-18:59"), value: 1.0 }] }, reply_style: MaiBotChatReplyStyleConfig { enable_reply_quote: true, group_chat_prompt: String::from("你正在qq群里聊天，下面是群里正在聊的内容，聊天中包含文字，图片和表情包等消息。\n回复尽量简短一些。最好一次对一个话题进行回复，但必须考虑不同群友发言之间的交互，免得啰嗦或者回复内容太乱。请注意把握聊天内容。\n不要总是提及自己的身份背景，根据聊天内容自由发挥，但是要日常不浮夸，不要刻意找话题。\n不用刻意回复其他人发送的表情包，只要关注表情包表达的含义。你可以适当发送表情包表达情绪。控制回复的频率，不要每个人的消息都回复，优先回复你感兴趣的或者主动提及你的，适当回复其他话题。\n"), private_chat_prompts: String::from("你正在聊天，下面是正在聊的内容，其中包含聊天记录和聊天中的图片。\n回复尽量简短一些。请注意把握聊天内容。\n请考虑对方的发言频率，想法，思考自己何时回复以及回复内容。\n"), chat_prompts: vec![] } },
            experimental: MaiBotExperimentalConfig { enable_behavior_learning: false, enable_rich_reply: false, emotion_trait: String::from("neutral"), attention_drift: MaiBotAttentionDriftConfig { enabled: false, drift_level: String::from("scattered"), anchor_policy: String::from("balanced"), reaction_style: String::from("lively") }, behavior_learning_list: vec![MaiBotLearningItem { platform: String::from(""), item_id: String::from(""), r#type: String::from("group"), r#use: true, learn: true }], behavior_groups: vec![], focus_mode: false, focus_on_private: false, focus_chat_whitelist: vec![], focus_groups: vec![], focus_cool_time: 120 },
            visual: MaiBotVisualConfig { planner_mode: String::from("auto"), replyer_mode: String::from("auto"), max_image_num: 128, wait_image_recognize_max_time: 10.0, handle_oversized_images: true, max_image_size_mb: 30.0, oversized_image_handle_method: String::from("compress"), image_cache_cleanup: MaiBotImageCacheCleanupConfig { enabled: true, check_interval_hours: 6.0, image_file_retention_days: 14, no_file_result_retention_days: 30 } },
            expression: MaiBotExpressionConfig { expression_checked_only: true, expression_self_reflect: true, expression_selection_mode: String::from("legacy"), expression_vector_index_path: String::from("data/expression_selection/expression_vector_index.json"), expression_vector_candidate_pool_size: 50, max_expression_learner: 3, learning_list: vec![MaiBotLearningItem { platform: String::from(""), item_id: String::from(""), r#type: String::from("group"), r#use: true, learn: true }], expression_groups: vec![] },
            jargon: MaiBotJargonConfig { learning_list: vec![MaiBotLearningItem { platform: String::from(""), item_id: String::from(""), r#type: String::from("group"), r#use: true, learn: true }], jargon_groups: vec![] },
            a_memorix: MaiBotAMemorixConfig { plugin: MaiBotAMemorixPluginConfig { enabled: false }, integration: MaiBotAMemorixIntegrationConfig { enable_memory_query_tool: true, memory_query_default_limit: 5, enable_person_profile_query_tool: true, enable_person_profile_injection: true, person_profile_injection_max_profiles: 3, heuristic_memory_recall_enabled: false, heuristic_memory_cross_chat_enabled: false, heuristic_memory_recall_window_size: 20, heuristic_memory_recall_limit: 3, heuristic_memory_recall_max_chars: 900, heuristic_memory_recall_min_interval_seconds: 180, heuristic_memory_recall_min_new_messages: 60, heuristic_memory_recall_cache_ttl_seconds: 300, heuristic_memory_group_to_private_enabled: false, heuristic_memory_private_to_group_enabled: false, person_fact_writeback_enabled: true, chat_summary_writeback_enabled: true, chat_summary_writeback_message_threshold: 36, chat_summary_writeback_context_length: 36, fuzzy_modify_enabled: true, fuzzy_modify_auto_execute_enabled: false, fuzzy_modify_confirm_threshold: 0.85, fuzzy_modify_candidate_limit: 20, fuzzy_modify_max_targets: 5, fuzzy_modify_allow_global_scope: false, feedback_correction_enabled: false, feedback_correction_window_hours: 12.0, feedback_correction_check_interval_minutes: 30, feedback_correction_batch_size: 20, feedback_correction_auto_apply_threshold: 0.85, feedback_correction_max_feedback_messages: 30, feedback_correction_prefilter_enabled: true, feedback_correction_paragraph_mark_enabled: true, feedback_correction_paragraph_hard_filter_enabled: true, feedback_correction_profile_refresh_enabled: true, feedback_correction_profile_force_refresh_on_read: true, feedback_correction_episode_rebuild_enabled: true, feedback_correction_episode_query_block_enabled: true, feedback_correction_reconcile_interval_minutes: 5, feedback_correction_reconcile_batch_size: 20 }, storage: MaiBotAMemorixStorageConfig { data_dir: String::from("data/a-memorix") }, embedding: MaiBotAMemorixEmbeddingConfig { model_name: String::from("auto"), dimension: 1024, dimension_request_mode: String::from("explicit"), batch_size: 32, max_concurrent: 5, enable_cache: false, runtime_train_threshold: 256, quantization_type: String::from("int8"), fallback: MaiBotAMemorixEmbeddingFallbackConfig { enabled: true, probe_interval_seconds: 180, allow_metadata_only_write: true }, paragraph_vector_backfill: MaiBotAMemorixParagraphVectorBackfillConfig { enabled: true, interval_seconds: 60, batch_size: 64, max_retry: 5 } }, retrieval: MaiBotAMemorixRetrievalConfig { top_k_paragraphs: 20, top_k_relations: 10, top_k_final: 10, alpha: 0.5, enable_ppr: true, ppr_alpha: 0.85, ppr_timeout_seconds: 1.5, ppr_concurrency_limit: 4, enable_parallel: true, search: MaiBotAMemorixRetrievalSearchConfig { smart_fallback: MaiBotAMemorixSmartFallbackConfig { enabled: true } }, fusion: MaiBotAMemorixFusionRetrievalConfig { method: String::from("weighted_rrf"), rrf_k: 60, vector_weight: 0.7, bm25_weight: 0.3 }, relation_vectorization: MaiBotAMemorixRelationVectorizationConfig { enabled: false, backfill_enabled: false, write_on_import: true }, vector_pools: MaiBotAMemorixVectorPoolsConfig { mode: String::from("dual"), paragraph_top_k: 20, graph_top_k: 40, graph_expand_paragraph_k: 80, relation_expand_per_hit: 5, entity_expand_per_hit: 8, relation_evidence_weight: 1.0, entity_evidence_weight: 0.55, semantic_weight: 0.65, sparse_weight: 0.2, graph_weight: 0.15, relation_intent: MaiBotAMemorixRelationIntentVectorPoolConfig { graph_top_k: 80, semantic_weight: 0.45, sparse_weight: 0.15, graph_weight: 0.4, return_relation_items: false } }, sparse: MaiBotAMemorixSparseRetrievalConfig { enabled: true, backend: String::from("fts5"), mode: String::from("auto"), tokenizer_mode: String::from("jieba"), candidate_k: 80, relation_candidate_k: 60 } }, threshold: MaiBotAMemorixThresholdConfig { min_threshold: 0.29, max_threshold: 0.95, percentile: 75, min_results: 4 }, filter: MaiBotAMemorixFilterConfig { enabled: true, mode: String::from("blacklist"), chats: vec![], retrieval: MaiBotAMemorixRetrievalFilterConfig { chat_stream: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] }, chat_summary: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] }, episode: MaiBotAMemorixRetrievalSubtypeFilterConfig { enabled: false, mode: String::from("blacklist"), chats: vec![] } } }, global_memory_sharing_enabled: false, shared_memory_groups: vec![], episode: MaiBotAMemorixEpisodeConfig { enabled: true, generation_enabled: true, source_poll_interval_seconds: 1.0, source_batch_size: 20, source_max_retry: 3, source_lease_seconds: 1800.0, source_max_wait_seconds: 60.0, max_paragraphs_per_call: 20, max_chars_per_call: 6000, source_time_window_hours: 24.0, segmentation_model: String::from("auto"), disabled_source_types: vec![String::from("person_fact")] }, person_profile: MaiBotAMemorixPersonProfileConfig { enabled: true, refresh_interval_minutes: 30, active_window_hours: 72.0, max_refresh_per_cycle: 50, refresh_debounce_seconds: 120, refresh_queue_interval_seconds: 60, refresh_queue_batch_size: 10, refresh_retry_backoff_seconds: 300, max_retry: 3, top_k_evidence: 12, evidence_classification_max_tokens: 1200, evidence_classification_temperature: 0.1 }, memory: MaiBotAMemorixMemoryEvolutionConfig { enabled: true, half_life_hours: 24.0, prune_threshold: 0.1, freeze_duration_hours: 24.0, revive_threshold: 0.15, access_reinforcement_alpha: 0.05, access_reinforcement_cooldown_minutes: 60.0, explicit_reinforcement_alpha: 0.5, weaken_alpha: 0.5, lifecycle_batch_size: 1000 }, advanced: MaiBotAMemorixAdvancedConfig { enable_auto_save: true, auto_save_interval_minutes: 5, debug: false }, web: MaiBotAMemorixWebConfig { import_config: MaiBotAMemorixWebImportConfig { enabled: true, max_queue_size: 20, max_files_per_task: 200, max_file_size_mb: 20, max_paste_chars: 200000, default_file_concurrency: 2, default_chunk_concurrency: 4, default_narrative_window_size: 1600, default_narrative_overlap: 400, default_factual_target_size: 1200, max_chunk_chars: 3200, timeout: MaiBotAMemorixWebImportTimeoutConfig { llm_call_seconds: 240.0, process_poll_seconds: 1.0, process_terminate_seconds: 5.0, process_kill_seconds: 3.0, convert_preflight_seconds: 20.0 } }, tuning: MaiBotAMemorixWebTuningConfig { enabled: true, max_queue_size: 8, poll_interval_ms: 1200, default_intensity: String::from("standard"), default_objective: String::from("precision_priority"), default_top_k_eval: 20, default_sample_size: 24 } } },
            message_receive: MaiBotMessageReceiveConfig { image_parse_threshold: 5, ban_words: vec![], ban_msgs_regex: vec![] },
            voice: MaiBotVoiceConfig { enable_asr: false },
            emoji: MaiBotEmojiConfig { emoji_send_num: 25, max_reg_num: 64, do_replace: true, check_interval: 10, steal_emoji: true, max_emoji_size_mb: 5.0, content_filtration: false, cache_cleanup: MaiBotEmojiCacheCleanupConfig { enabled: true, check_interval_hours: 6.0, emoji_file_retention_days: 30, no_file_record_retention_days: 30 } },
            keyword_reaction: MaiBotKeywordReactionConfig { keyword_rules: vec![], regex_rules: vec![] },
            response_post_process: MaiBotResponsePostProcessConfig { enable_response_post_process: true, typing_speed: 1.0 },
            chinese_typo: MaiBotChineseTypoConfig { enable: true, enable_correction_quote: true, correction_quote_probability: 1.0, error_rate: 0.01, min_freq: 9, tone_error_rate: 0.1, word_replace_rate: 0.006 },
            response_splitter: MaiBotResponseSplitterConfig { enable: true, max_length: 512, max_sentence_num: 8, max_split_num: 3, enable_kaomoji_protection: false, enable_overflow_return_all: false },
            telemetry: MaiBotTelemetryConfig { enable: true },
            log: MaiBotLogConfig { date_style: String::from("m-d H:i:s"), log_level_style: String::from("lite"), color_text: String::from("full"), log_level: String::from("INFO"), console_log_level: String::from("INFO"), file_log_level: String::from("DEBUG"), log_file_max_bytes: 5242880, max_log_files: 30, log_cleanup_days: 30, llm_request_snapshot_limit: 128, maisaka_prompt_preview_limit: 256, maisaka_reply_effect_limit: 256, suppress_libraries: vec![String::from("faiss"), String::from("httpx"), String::from("urllib3"), String::from("asyncio"), String::from("websockets"), String::from("httpcore"), String::from("requests"), String::from("sqlalchemy"), String::from("openai"), String::from("uvicorn"), String::from("jieba")], library_log_levels: std::collections::BTreeMap::from([(String::from("aiohttp"), String::from("WARNING")), (String::from("PIL"), String::from("WARNING"))]) },
            debug: MaiBotDebugConfig { enable_console_input: false, show_maisaka_thinking: true, enable_clear_context_command: false, enable_reply_effect_tracking: false, keep_prompt_preview_json_base64: false, record_tool_structured_content: false, enable_llm_cache_stats: false },
            maim_message: MaiBotMaimMessageConfig { ws_server_host: String::from("127.0.0.1"), ws_server_port: 8000, auth_token: vec![], enable_api_server: false, api_server_host: String::from("0.0.0.0"), api_server_port: 8090, api_server_use_wss: false, api_server_cert_file: String::from(""), api_server_key_file: String::from(""), api_server_allowed_api_keys: vec![] },
            webui: MaiBotWebUIConfig { enabled: true, host: vec![String::from("127.0.0.1"), String::from("::1")], port: 8001, mode: String::from("production"), webui_style: 1, anti_crawler_mode: String::from("basic"), allowed_ips: String::from("127.0.0.1"), trusted_proxies: String::from(""), trust_xff: false, secure_cookie: false, enforce_public_outbound_url: true, enable_paragraph_content: false },
            database: MaiBotDatabaseConfig { save_binary_data: false },
            mcp: MaiBotMCPConfig { enable: true, client: MaiBotMCPClientConfig { client_name: String::from("MaiBot"), client_version: String::from("1.0.0"), roots: MaiBotMCPRootsConfig { enable: false, items: vec![] }, sampling: MaiBotMCPSamplingConfig { enable: false, task_name: String::from("planner"), include_context_support: false, tool_support: false }, elicitation: MaiBotMCPElicitationConfig { enable: false, allow_form: true, allow_url: false } }, servers: vec![] },
            plugin: MaiBotPluginConfig { permission: vec![], command_permissions: std::collections::BTreeMap::new() },
            plugin_runtime: MaiBotPluginRuntimeConfig { enabled: true, health_check_interval_sec: 30.0, max_restart_attempts: 3, runner_spawn_timeout_sec: 30.0, hook_blocking_timeout_sec: 60.0, ipc_socket_path: String::from(""), render: MaiBotPluginRuntimeRenderConfig { enabled: true, browser_ws_endpoint: String::from(""), executable_path: String::from(""), browser_install_root: String::from("data/playwright-browsers"), headless: true, launch_args: vec![String::from("--disable-gpu"), String::from("--disable-dev-shm-usage"), String::from("--disable-setuid-sandbox"), String::from("--no-sandbox"), String::from("--no-zygote")], concurrency_limit: 2, startup_timeout_sec: 20.0, render_timeout_sec: 15.0, auto_download_chromium: true, download_connection_timeout_sec: 120.0, restart_after_render_count: 200 } },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotModelInfo {
    /// 模型标识符 (API服务商提供的模型标识符)
    pub model_identifier: String,
    /// 模型名称 (可随意命名, 在models中需使用这个命名)
    pub name: String,
    /// API服务商名称 (对应在api_providers中配置的服务商名称)
    pub api_provider: String,
    /// 输入价格 (用于API调用统计, 单位：元/ M token) (可选, 若无该字段, 默认值为0)
    pub price_in: f64,
    /// 是否启用模型输入缓存计费。开启后命中缓存的输入 token 使用 cache_price_in 计费。
    pub cache: bool,
    /// 缓存命中输入价格 (用于API调用统计, 单位：元/ M token)。仅当 cache=true 时使用。
    pub cache_price_in: f64,
    /// 输出价格 (用于API调用统计, 单位：元/ M token) (可选, 若无该字段, 默认值为0)
    pub price_out: f64,
    /// 模型级别温度（可选），会覆盖任务配置中的温度
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub temperature: Option<f64>,
    /// 是否向模型服务发送由 MaiBot 管理的 temperature 参数。
    pub send_temperature: bool,
    /// 模型级别最大token数（可选），会覆盖任务配置中的max_tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number")]
    pub max_tokens: Option<i64>,
    /// 强制流式输出模式 (若模型不支持非流式输出, 请设置为true启用强制流式输出, 默认值为false)
    pub force_stream_mode: bool,
    /// 是否为多模态模型。开启后表示该模型支持视觉输入。
    pub visual: bool,
    /// 额外参数 (用于API调用时的额外配置)。
    pub extra_params: String,
}

impl MaiBotModelInfo {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/price_in"), self.price_in, Some(0.0), None);
        sink.range(&format!("{path}/cache_price_in"), self.cache_price_in, Some(0.0), None);
        sink.range(&format!("{path}/price_out"), self.price_out, Some(0.0), None);
    }
}

impl Default for MaiBotModelInfo {
    fn default() -> Self {
        Self {
            model_identifier: String::from(""),
            name: String::from(""),
            api_provider: String::from(""),
            price_in: 0.0,
            cache: false,
            cache_price_in: 0.0,
            price_out: 0.0,
            temperature: None,
            send_temperature: true,
            max_tokens: None,
            force_stream_mode: false,
            visual: false,
            extra_params: String::from("{}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotTaskConfig {
    /// 使用的模型列表, 每个元素对应上面的模型名称(name)
    pub model_list: Vec<String>,
    /// 任务最大输出token数
    #[ts(type = "number")]
    pub max_tokens: i64,
    /// 模型温度
    pub temperature: f64,
    /// 超时警告时间（秒），超过此时间会输出警告日志
    pub slow_threshold: f64,
    /// 模型选择策略：balance（负载均衡）、random（随机选择）或 sequential（按配置顺序优先选择）
    pub selection_strategy: String,
    /// 任务硬超时（秒），到点未返回则取消请求并尝试切换下一个模型；防止上游代理静默排队导致主循环饥饿
    pub hard_timeout: f64,
}

impl MaiBotTaskConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/max_tokens"), self.max_tokens as f64, Some(1.0), None);
        sink.range(&format!("{path}/temperature"), self.temperature, Some(0.0), Some(2.0));
        sink.range(&format!("{path}/slow_threshold"), self.slow_threshold, Some(0.0), None);
        sink.range(&format!("{path}/hard_timeout"), self.hard_timeout, Some(1.0), None);
    }
}

impl Default for MaiBotTaskConfig {
    fn default() -> Self {
        Self {
            model_list: vec![],
            max_tokens: 4096,
            temperature: 0.3,
            slow_threshold: 15.0,
            selection_strategy: String::from("balance"),
            hard_timeout: 240.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotModelTaskConfig {
    /// 回复模型，影响麦麦的回复表现
    pub replyer: MaiBotTaskConfig,
    /// 规划模型，决定麦麦的行动，需要有一定Agent能力的模型
    pub planner: MaiBotTaskConfig,
    /// 记忆模型配置，用于长期记忆总结、抽取、写回等高记忆任务；留空时由调用方按需回退
    pub memory: MaiBotTaskConfig,
    /// 聊天回想模型配置；留空时自动继用 planner 模型
    pub mid_memory: MaiBotTaskConfig,
    /// 执行文本概括，整理等小任务，是麦麦必须的模型。可以选择速度快的小尺寸模型
    pub utils: MaiBotTaskConfig,
    /// 学习模型配置，用于表达方式学习和黑话学习；留空时用 utils 模型
    pub learner: MaiBotTaskConfig,
    /// 表达方式使用模型配置；留空时用 utils 模型
    pub expression_use: MaiBotTaskConfig,
    /// 表情包发送模型配置；留空时保持原有 planner/vlm 选择逻辑
    pub emoji: MaiBotTaskConfig,
    /// 视觉模型，需要能够识图的模型
    pub vlm: MaiBotTaskConfig,
    /// 语音识别模型
    pub voice: MaiBotTaskConfig,
    /// 嵌入模型，需要文本嵌入类型的模型，不可使用LLM
    pub embedding: MaiBotTaskConfig,
}

impl MaiBotModelTaskConfig {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        self.replyer.validate(&format!("{path}/replyer"), sink);
        self.planner.validate(&format!("{path}/planner"), sink);
        self.memory.validate(&format!("{path}/memory"), sink);
        self.mid_memory.validate(&format!("{path}/mid_memory"), sink);
        self.utils.validate(&format!("{path}/utils"), sink);
        self.learner.validate(&format!("{path}/learner"), sink);
        self.expression_use.validate(&format!("{path}/expression_use"), sink);
        self.emoji.validate(&format!("{path}/emoji"), sink);
        self.vlm.validate(&format!("{path}/vlm"), sink);
        self.voice.validate(&format!("{path}/voice"), sink);
        self.embedding.validate(&format!("{path}/embedding"), sink);
    }
}

impl Default for MaiBotModelTaskConfig {
    fn default() -> Self {
        Self {
            replyer: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
            planner: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
            memory: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
            mid_memory: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
            utils: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
            learner: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
            expression_use: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
            emoji: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
            vlm: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
            voice: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
            embedding: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotAPIProvider {
    /// API服务商名称 (可随意命名, 在models的api-provider中需使用这个命名)
    pub name: String,
    /// API服务商的BaseURL
    pub base_url: String,
    /// API密钥。对于不需要鉴权的兼容端点，可将 `auth_type` 设为 `none`。
    pub api_key: String,
    /// 客户端类型。内置支持 openai、openai_responses 和 gemini，也可由插件扩展。
    pub client_type: String,
    /// OpenAI 兼容接口的鉴权方式。可选值：`bearer`、`header`、`query`、`none`。
    pub auth_type: String,
    /// 当 `auth_type` 为 `header` 时使用的请求头名称。
    pub auth_header_name: String,
    /// 当 `auth_type` 为 `header` 时使用的请求头前缀。留空表示直接发送原始密钥。
    pub auth_header_prefix: String,
    /// 当 `auth_type` 为 `query` 时使用的查询参数名称。
    pub auth_query_name: String,
    /// 所有请求默认附带的 HTTP Header。
    pub default_headers: std::collections::BTreeMap<String, String>,
    /// 所有请求默认附带的查询参数。
    pub default_query: std::collections::BTreeMap<String, String>,
    /// OpenAI 官方接口可选的 `organization`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub organization: Option<String>,
    /// OpenAI 官方接口可选的 `project`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project: Option<String>,
    /// 模型列表端点路径。适用于 OpenAI 兼容接口的探测与管理。
    pub model_list_endpoint: String,
    /// 推理内容解析模式。可选值：`auto`、`native`、`think_tag`、`none`。
    pub reasoning_parse_mode: String,
    /// 工具参数解析模式。可选值：`auto`、`strict`、`repair`、`double_decode`。
    pub tool_argument_parse_mode: String,
    /// 最大重试次数 (单个模型API调用失败, 最多重试的次数)
    #[ts(type = "number")]
    pub max_retry: i64,
    /// API调用的超时时长 (超过这个时长, 本次请求将被视为"请求超时", 单位: 秒)
    #[ts(type = "number")]
    pub timeout: i64,
    /// 重试间隔 (如果API调用失败, 重试的间隔时间, 单位: 秒)
    #[ts(type = "number")]
    pub retry_interval: i64,
}

impl MaiBotAPIProvider {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        sink.range(&format!("{path}/max_retry"), self.max_retry as f64, Some(0.0), None);
        sink.range(&format!("{path}/timeout"), self.timeout as f64, Some(1.0), None);
        sink.range(&format!("{path}/retry_interval"), self.retry_interval as f64, Some(1.0), None);
    }
}

impl Default for MaiBotAPIProvider {
    fn default() -> Self {
        Self {
            name: String::from(""),
            base_url: String::from(""),
            api_key: String::from(""),
            client_type: String::from("openai"),
            auth_type: String::from("bearer"),
            auth_header_name: String::from("Authorization"),
            auth_header_prefix: String::from("Bearer"),
            auth_query_name: String::from("api_key"),
            default_headers: std::collections::BTreeMap::new(),
            default_query: std::collections::BTreeMap::new(),
            organization: None,
            project: None,
            model_list_endpoint: String::from("/models"),
            reasoning_parse_mode: String::from("auto"),
            tool_argument_parse_mode: String::from("auto"),
            max_retry: 3,
            timeout: 60,
            retry_interval: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/maibot/")]
pub struct MaiBotModelConfigFile {
    /// 模型配置列表
    pub models: Vec<MaiBotModelInfo>,
    /// 模型任务配置
    pub model_task_config: MaiBotModelTaskConfig,
    /// API提供商列表
    pub api_providers: Vec<MaiBotAPIProvider>,
}

impl MaiBotModelConfigFile {
    pub fn validate(&self, path: &str, sink: &mut IssueSink) {
        for (i, item) in self.models.iter().enumerate() {
            item.validate(&format!("{path}/models/{i}"), sink);
        }
        self.model_task_config.validate(&format!("{path}/model_task_config"), sink);
        for (i, item) in self.api_providers.iter().enumerate() {
            item.validate(&format!("{path}/api_providers/{i}"), sink);
        }
    }
}

impl Default for MaiBotModelConfigFile {
    fn default() -> Self {
        Self {
            models: vec![],
            model_task_config: MaiBotModelTaskConfig { replyer: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 }, planner: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 }, memory: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 }, mid_memory: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 }, utils: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 }, learner: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 }, expression_use: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 }, emoji: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 }, vlm: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 }, voice: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 }, embedding: MaiBotTaskConfig { model_list: vec![], max_tokens: 4096, temperature: 0.3, slow_threshold: 15.0, selection_strategy: String::from("balance"), hard_timeout: 240.0 } },
            api_providers: vec![],
        }
    }
}

