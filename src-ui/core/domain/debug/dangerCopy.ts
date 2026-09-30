// 危险接口发送前的确认框：把「这次调用会发生什么」用具体的参数说清楚。
// 分级本身在后端补充表里；这里只管文案，表里没有专门句子的危险接口走兜底句。

const FALLBACK = '这个接口会对 QQ 产生不可撤销的影响';

/** 号类参数：0 不是真的号，是没填（老标签、别处带进来的参数里可能还留着 0 占位） */
const ID_KEYS: ReadonlySet<string> = new Set(['group_id', 'user_id', 'message_id']);

/** 参数值取出来给人看；缺了就用占位，让用户看得出「这里没填」而不是悄悄漏掉 */
function show(params: Record<string, unknown>, key: string): string {
    const v = params[key];
    if (v === undefined || v === null || v === '') return `（未填 ${key}）`;
    if (ID_KEYS.has(key) && (v === 0 || (typeof v === 'string' && v.trim() === '0'))) return `（未填 ${key}）`;
    return typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean' ? String(v) : JSON.stringify(v);
}

/** NapCat 的布尔参数可能是字符串 "true" / "false"，缺省时用 fallback */
function flag(params: Record<string, unknown>, key: string, fallback: boolean): boolean {
    const v = params[key];
    if (v === undefined || v === null || v === '') return fallback;
    if (typeof v === 'boolean') return v;
    if (typeof v === 'number') return v !== 0;
    if (typeof v === 'string') return !['false', '0', 'no'].includes(v.trim().toLowerCase());
    return fallback;
}

export function dangerConsequence(action: string, params: Record<string, unknown>, botName: string): string {
    switch (action) {
        case 'bot_exit':
            return `${botName} 会退出登录，需要重新登录`;
        case 'set_restart':
            return `${botName} 会重启`;
        case 'set_group_kick':
            return `会把 ${show(params, 'user_id')} 移出群 ${show(params, 'group_id')}`;
        case 'set_group_kick_members': {
            const ids = params.user_id;
            const count = Array.isArray(ids) ? ids.length : 0;
            return count > 0
                ? `会把 ${count} 个人移出群 ${show(params, 'group_id')}`
                : `会批量移出群 ${show(params, 'group_id')} 的成员`;
        }
        case 'set_group_ban': {
            const duration = params.duration;
            const unset = duration === undefined || duration === null || (typeof duration === 'string' && duration.trim() === '');
            // 没填时上游会按默认的 30 分钟禁言，绝不是解除；只有明确写了 0 才是解除禁言
            if (unset) return `会禁言 ${show(params, 'user_id')} （未填 duration，上游默认 1800 秒）`;
            if (Number(duration) === 0) {
                return `会解除 ${show(params, 'user_id')} 在群 ${show(params, 'group_id')} 的禁言`;
            }
            return `会禁言 ${show(params, 'user_id')} ${show(params, 'duration')} 秒`;
        }
        case 'set_group_whole_ban':
            return flag(params, 'enable', true)
                ? `会对群 ${show(params, 'group_id')} 开启全员禁言`
                : `会关闭群 ${show(params, 'group_id')} 的全员禁言`;
        case 'set_group_leave':
            return flag(params, 'is_dismiss', false)
                ? `会解散群 ${show(params, 'group_id')}（仅群主可以）`
                : `会退出群 ${show(params, 'group_id')}`;
        case 'set_group_admin':
            return flag(params, 'enable', true)
                ? `会把 ${show(params, 'user_id')} 设为群 ${show(params, 'group_id')} 的管理员`
                : `会取消 ${show(params, 'user_id')} 在群 ${show(params, 'group_id')} 的管理员`;
        case 'delete_msg':
            return `会撤回消息 ${show(params, 'message_id')}`;
        case 'delete_friend':
            return `会删除好友 ${show(params, 'user_id')}`;
        case 'clean_cache':
            return `会清空 ${botName} 的缓存文件`;
        case 'delete_group_file':
            return `会删除群 ${show(params, 'group_id')} 里的文件 ${show(params, 'file_id')}`;
        case 'delete_group_folder':
            return `会删除群 ${show(params, 'group_id')} 里的文件夹 ${show(params, 'folder_id')}`;
        case '_del_group_notice':
            return `会删除群 ${show(params, 'group_id')} 的一条公告`;
        case 'delete_flash_file':
            return '会删除这个闪传文件';
        default:
            return FALLBACK;
    }
}
