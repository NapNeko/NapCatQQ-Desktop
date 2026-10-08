// 文件、表情、流式、扩展与预览专用动作的定义。

import { type ActionDef, arr, gid, obj, T, COMMON_ERRORS } from './catalog-schema';

export const DEFS_FILE: ActionDef[] = [
    // —— 文件
    {
        name: 'upload_group_file',
        summary: '上传群文件',
        category: 'file',
        safety: 'side_effect',
        params: [
            gid(),
            {
                name: 'file',
                role: 'file',
                type: 'string',
                desc: 'Bot 所在机器上的文件路径，或 http(s) 地址',
                required: true,
            },
            { name: 'name', type: 'string', desc: '文件在群里显示的名字', required: true },
            {
                name: 'folder_id',
                type: 'string',
                desc: '目标文件夹 ID，不填为根目录',
                only: 'napcat',
            },
            {
                name: 'folder',
                type: 'string',
                desc: '目标文件夹 ID，不填为根目录',
                only: 'snowluma',
            },
        ],
        returns: T.nul,
        example: { group_id: '100001', file: '/tmp/report.pdf', name: 'report.pdf' },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '文件不存在' }, ...COMMON_ERRORS],
    },
    {
        name: 'get_group_file_url',
        summary: '获取群文件下载链接',
        category: 'file',
        safety: 'read_only',
        params: [
            gid(),
            { name: 'file_id', type: 'string', desc: '文件 ID', required: true },
            { name: 'busid', type: 'integer', desc: '文件类型 ID', default: 102 },
        ],
        returns: obj({ url: T.str }),
        example: { group_id: '100001', file_id: '/abcd-1234' },
        returnData: { url: 'https://example.invalid/file/abcd-1234' },
        errorExamples: [{ retcode: 1200, message: '文件不存在' }, ...COMMON_ERRORS],
    },
    {
        name: 'delete_group_file',
        summary: '删除群文件',
        category: 'file',
        safety: 'dangerous',
        params: [
            gid(),
            { name: 'file_id', type: 'string', desc: '文件 ID', required: true },
            { name: 'busid', type: 'integer', desc: '文件类型 ID', default: 102 },
        ],
        returns: T.nul,
        example: { group_id: '100001', file_id: '/abcd-1234' },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '权限不足或文件不存在' }, ...COMMON_ERRORS],
        invariants: ['只能删除自己上传的文件，除非有管理权限'],
    },
    {
        // 两边把「目标目录」写成了不同的参数名：照 NC 的 `target_parent_directory` 发到 SL 上，
        // SL 要的 `target_directory` 没填，会直接失败 —— 目录行上带「参数不同」徽章的典型
        name: 'move_group_file',
        summary: '移动群文件',
        category: 'file',
        safety: 'side_effect',
        params: [
            gid(),
            { name: 'file_id', type: 'string', desc: '文件 ID', required: true },
            { name: 'current_parent_directory', type: 'string', desc: '当前目录', required: true },
            {
                name: 'target_parent_directory',
                type: 'string',
                desc: '目标目录',
                required: true,
                only: 'napcat',
            },
            {
                name: 'target_directory',
                type: 'string',
                desc: '目标目录',
                required: true,
                only: 'snowluma',
            },
        ],
        returns: T.nul,
        example: {
            group_id: '100001',
            file_id: '/abcd-1234',
            current_parent_directory: '/',
            target_parent_directory: '/docs',
        },
        returnData: null,
        errorExamples: [{ retcode: 1200, message: '文件不存在' }, ...COMMON_ERRORS],
    },
];

export const DEFS_FACE: ActionDef[] = [
    // —— 表情
    {
        name: 'fetch_custom_face',
        summary: '获取收藏的自定义表情',
        category: 'face',
        safety: 'read_only',
        params: [
            {
                name: 'count',
                type: 'integer',
                desc: '最多返回几个',
                default: 48,
                minimum: 1,
                maximum: 200,
            },
        ],
        returns: arr(T.str),
        returnData: ['https://example.invalid/face/1.png'],
        errorExamples: COMMON_ERRORS,
    },
];

export const DEFS_STREAM: ActionDef[] = [
    // —— 流式
    {
        name: 'upload_file_stream',
        summary: '分片上传文件（流式）',
        description:
            '把大文件切片后逐片发给 Bot，再由 Bot 合并落盘。本期调试台只提供文档，调用按钮置灰。',
        category: 'stream',
        safety: 'side_effect',
        stream: true,
        params: [
            {
                name: 'stream_id',
                type: 'string',
                desc: '同一个文件的分片共用一个 ID',
                required: true,
            },
            { name: 'chunk_data', type: 'string', desc: '本片内容（base64）' },
            { name: 'chunk_index', type: 'integer', desc: '本片序号，从 0 起' },
            { name: 'total_chunks', type: 'integer', desc: '总片数' },
            { name: 'file_size', type: 'integer', desc: '文件总字节数' },
            { name: 'filename', type: 'string', desc: '落盘文件名' },
        ],
        returns: obj({ type: T.str, stream_id: T.str, status: T.str, received_chunks: T.int }),
        returnData: {
            type: 'stream',
            stream_id: 's1',
            status: 'chunk_received',
            received_chunks: 1,
        },
        errorExamples: COMMON_ERRORS,
    },
];

export const DEFS_EXTENSION: ActionDef[] = [
    // —— 扩展（各自独有）
    {
        name: 'nc_get_rkey',
        summary: '获取图片直链所需的 rkey',
        category: 'extension',
        safety: 'read_only',
        backends: ['napcat'],
        params: [],
        returns: arr(obj({ type: T.str, rkey: T.str, created_at: T.int, ttl: T.int })),
        returnData: [
            { type: 'private', rkey: '&rkey=CAQSKAB6JW...', created_at: 1759190400, ttl: 86400 },
        ],
        errorExamples: COMMON_ERRORS,
    },
    {
        name: 'get_group_album_list',
        summary: '获取群相册列表',
        category: 'extension',
        safety: 'read_only',
        backends: ['snowluma'],
        params: [gid()],
        returns: arr(obj({ album_id: T.str, name: T.str, upload_number: T.int })),
        returnsText: '数组：每项含 album_id、name、upload_number',
        returnData: [{ album_id: 'album_1', name: '聚会', upload_number: 12 }],
        invariants: ['Bot 必须在这个群里'],
    },
];

export const DEFS_PREVIEW: ActionDef[] = [
    // —— 预览专用：真上游没有这个动作，只为在浏览器里走一遍超大回包被截断的界面
    {
        name: 'debug_huge_response',
        summary: '预览用：回一个超过 5 MiB 的回包',
        description:
            '浏览器预览专用，真 Bot 上没有这个动作。回包约 5.7 MiB，超过调试台 5 MiB 的内联上限：结果区只显示前 256 KiB 的文本预览，完整内容用「另存完整内容」保存。',
        category: 'extension',
        safety: 'read_only',
        params: [],
        returns: obj({
            note: T.str,
            rows: arr(
                obj({ seq: T.int, group_id: T.int, user_id: T.int, nickname: T.str, text: T.str }),
            ),
        }),
        returnsText: '对象：note，以及约五万行的 rows',
        returnData: {
            note: '预览用的超大回包：结果区只显示前 256 KiB，完整内容请另存',
            rows: [
                {
                    seq: 1,
                    group_id: 100001,
                    user_id: 10001,
                    nickname: '小明',
                    text: '第 1 行：预览用的填充数据',
                },
            ],
        },
        errorExamples: COMMON_ERRORS,
        invariants: ['只在浏览器预览里有', '回包超过 5 MiB，调试台只给前 256 KiB 的预览'],
    },
];
