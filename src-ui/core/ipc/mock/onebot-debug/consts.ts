// 调试台预览假后端的公共常量：事件协议版本、缓冲与回包体积上限、延迟节奏、种子时间戳。

export const EVENT_VERSION = 1;
/** 每个 Bot 的事件缓冲上限，和后端一致 */
export const RING_CAP = 5000;
/** 超出上限多少条才裁一次：一次裁一批并写一条 dropped，不是每来一条就写 */
export const RING_TRIM_SLACK = 100;
export const BACKLOG_CHUNK = 500;
export const BATCH_FLUSH_MS = 50;
export const DEFAULT_TIMEOUT_MS = 60_000;
export const MEMBER_LIST_DELAY_MS = 900;
/** 连通测试要走一趟上游，这么久之后才出结果 */
export const TEST_CHANNEL_MS = 400;
export const HEARTBEAT_EVERY_MS = 30_000;
/** 后端清扫的间隔：每一轮给每个窗口发一个空批次，探窗口还在不在 */
export const PROBE_EVERY_MS = 60_000;
/** 回包文本超过这么大就不整块给界面，和后端 RESPONSE_INLINE_LIMIT 一致 */
export const RESPONSE_INLINE_LIMIT = 5 * 1024 * 1024;
/** 截断时给界面的文本预览字节数，和后端 RAW_PREVIEW_LIMIT 一致 */
export const RAW_PREVIEW_LIMIT = 256 * 1024;
/** 单条历史里回包序列化后的上限，超了回包整个不存、只留「被截断」标记，和后端一致 */
export const HISTORY_RESPONSE_LIMIT = 256 * 1024;
/** 被截断的回包全文只留最近这么多次，够「另存完整内容」用，和后端一致 */
export const LARGE_RESPONSES_KEPT = 3;
/** 「超大回包」动作的行数：每行一百多字节，合起来约 5.7 MiB，稳稳越过 5 MiB 的内联上限 */
export const HUGE_RESPONSE_ROWS = 50_000;
export const MESSAGE_STORE_CAP = 3000;
export const HISTORY_CAP = 1000;
/**
 * 回包里是登录凭据的动作：后端的历史只记「调过」、不记回包（`_async` 变体同理），
 * 预览照做，免得界面在预览里显示出真程序永远不会有的回包
 */
export const CREDENTIAL_ACTIONS: ReadonlySet<string> = new Set([
    'get_cookies',
    'get_credentials',
    'get_csrf_token',
    'get_clientkey',
    'get_rkey',
    'nc_get_rkey',
    'get_rkey_server',
]);
/** 收藏夹种子的时间戳：固定值，不让预览数据随打开时间飘 */
export const SEED_EPOCH_MS = 1_759_190_400_000;
export const FIRST_MESSAGE_ID = 1_700_000_000;

export type Timer = ReturnType<typeof setTimeout>;

export const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
export const isRecord = (v: unknown): v is Record<string, unknown> =>
    typeof v === 'object' && v !== null && !Array.isArray(v);
