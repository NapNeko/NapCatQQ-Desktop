// QQ 数字表情的内置目录；来源 QFace _index.v2.json，2026-09-28 快照。
import type { Segment } from '../debug/segments';
export interface QQSystemFace {
    id: string;
    name: string;
    aliases?: string[];
    url?: string;
    category?: string;
    super?: boolean;
    animationUrl?: string;
}
// QQNT emojiType 1/2/3/5 的数字表情快照；账号目录可以覆盖它。
export const QQ_SUPER_FACE_IDS = new Set(
    '5,53,74,75,114,137,181,311,312,314,317,318,319,320,324,325,326,333,337,338,339,341,342,343,344,345,346,349,350,351,358,359,360,361,362,363,364,365,366,367,368,369,370,371,372,373,374,375,376,377,378,379,380,381,382,383,384,385,386,387,388,389,390,391,392,393,394,395,396,397,398,399,400,401,402,403,404,405,406,407,408,409,410,411,412,413,415,416,417,418,419,420,421,422,423,424,425,426,427,429,430,431,432,433,434,435,436,437,438,439,440,441,442,443,444,445,446,447,448,450,451,452,453,454,455,456,457,458,459,460,461,462,463,464,465,466,467,468,469,471,472,473,474,475,476,477,478,479,480,481,482,483,484,485,486,487,488,489,490,491,492,493,494,495,496,497,498,499,500,501,502,503,504,505,506,507'.split(
        ',',
    ),
);
const interactiveIds = new Set(
    '114,358,359,392,393,394,415,416,417,419,420,421,429,430,431,443,444,445,446,447,448,485,486,487'.split(
        ',',
    ),
);
export function qqFaceCategory(face: QQSystemFace): string {
    return (
        face.category ||
        (interactiveIds.has(face.id)
            ? '互动'
            : (face.super ?? QQ_SUPER_FACE_IDS.has(face.id))
              ? '超级'
              : '经典')
    );
}
export function qqFaceLarge(data?: Record<string, unknown>): boolean | undefined {
    if (
        data?.large === false ||
        data?.large === 0 ||
        data?.large === '0' ||
        data?.large === 'false'
    )
        return false;
    if (data?.large === true || data?.large === 1 || data?.large === '1' || data?.large === 'true')
        return true;
    const raw = data?.raw;
    if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
        const faceType = Number((raw as Record<string, unknown>).faceType);
        if (faceType === 3) return true;
        if (faceType === 1 || faceType === 2) return false;
    }
    if (data?.faceType != null) {
        const faceType = Number(data.faceType);
        if (faceType === 3) return true;
        if (faceType === 1 || faceType === 2) return false;
    }
    if (typeof data?.is_super === 'boolean') return data.is_super;
    // 目录描述的是可用资源，不能证明这条消息用了超级表情格式。
    return undefined;
}
export function isSuperQQFace(_id: string, data?: Record<string, unknown>): boolean {
    return qqFaceLarge(data) === true;
}
export type QQFaceDisplaySegment = Segment & { displayLarge?: boolean };
type FaceLookup = (id: string) => QQSystemFace | undefined;
// 2026 秋季新表情的独立展示名单；不作为无标记消息的协议类型事实。
const standaloneDisplayIds = new Set(
    '494,495,496,497,498,499,500,501,502,503,504,505,506,507'.split(','),
);
function facePlaceholders(name: string): string[] {
    const label = name.replace(/^\/+/, '').trim();
    if (!label) return [];
    if (label.startsWith('[') && label.endsWith(']')) return [label];
    return label.endsWith('!') ? ['[' + label + ']'] : ['[' + label + ']', '[' + label + '!]'];
}
export function projectQQFaceDisplay(
    segments: readonly Segment[],
    lookup: FaceLookup = (id) => QQ_FACE_FALLBACK.find((face) => face.id === id),
): QQFaceDisplaySegment[] {
    const knownFace = (segment: Segment) => {
        const id = String(segment.data.id ?? '');
        return lookup(id) ?? QQ_FACE_FALLBACK.find((face) => face.id === id);
    };
    const knownSuper = (segment: Segment) => {
        const face = knownFace(segment);
        return !!face && (face.super ?? QQ_SUPER_FACE_IDS.has(face.id));
    };
    const knownCaption = (segment: Segment, next: Segment | undefined) => {
        const face = knownFace(segment);
        return (
            !!face &&
            next?.type === 'text' &&
            typeof next.data.text === 'string' &&
            [face.name, ...(face.aliases ?? [])].some((name) =>
                facePlaceholders(name).includes(next.data.text as string),
            )
        );
    };
    const body = segments.filter((segment) => segment.type !== 'reply');
    const singleKnownSuper =
        body.length === 1 &&
        body[0].type === 'face' &&
        standaloneDisplayIds.has(String(body[0].data.id ?? '')) &&
        knownSuper(body[0]);
    let onlyFacePairs = body.length > 1;
    for (let index = 0; index < body.length && onlyFacePairs; index += 2) {
        const face = body[index];
        onlyFacePairs =
            face.type === 'face' &&
            qqFaceLarge(face.data) !== false &&
            knownSuper(face) &&
            knownCaption(face, body[index + 1]);
    }
    const allowCompatibilityDisplay = singleKnownSuper || onlyFacePairs;
    const result: QQFaceDisplaySegment[] = [];
    for (let index = 0; index < segments.length; index++) {
        const segment = segments[index];
        if (segment.type !== 'face') {
            result.push(segment);
            continue;
        }
        const wireLarge = qqFaceLarge(segment.data);
        const displayLarge = wireLarge ?? (allowCompatibilityDisplay && knownSuper(segment));
        // 推断只留在投影外层，原 data、草稿与再次发送的 wire 不变。
        result.push(
            wireLarge === undefined && displayLarge ? { ...segment, displayLarge: true } : segment,
        );
        if (!displayLarge) continue;
        const next = segments[index + 1];
        if (wireLarge === undefined) {
            if (knownCaption(segment, next)) index++;
            continue;
        }
        const raw = segment.data.raw;
        const metadata =
            raw && typeof raw === 'object' && !Array.isArray(raw)
                ? (raw as Record<string, unknown>)
                : segment.data;
        const name = typeof metadata.faceText === 'string' ? metadata.faceText : '';
        const captions = name.startsWith('/')
            ? [name, '[' + name.slice(1) + ']', '[' + name.slice(1) + '!]']
            : [name];
        // 只折叠真实 faceText 的一个完整邻接副本，不删混合正文或未标记的普通表情文本。
        if (name && next?.type === 'text' && captions.includes(String(next.data.text ?? '')))
            index++;
    }
    return result;
}
export const QQ_FACE_FALLBACK: QQSystemFace[] = [
    { id: '0', name: '惊讶' },
    { id: '1', name: '撇嘴' },
    { id: '2', name: '色' },
    { id: '3', name: '发呆' },
    { id: '4', name: '得意' },
    { id: '5', name: '流泪' },
    { id: '6', name: '害羞' },
    { id: '7', name: '闭嘴' },
    { id: '8', name: '睡' },
    { id: '9', name: '大哭' },
    { id: '10', name: '尴尬' },
    { id: '11', name: '发怒' },
    { id: '12', name: '调皮' },
    { id: '13', name: '呲牙' },
    { id: '14', name: '微笑' },
    { id: '15', name: '难过' },
    { id: '16', name: '酷' },
    { id: '18', name: '抓狂' },
    { id: '19', name: '吐' },
    { id: '20', name: '偷笑' },
    { id: '21', name: '可爱' },
    { id: '22', name: '白眼' },
    { id: '23', name: '傲慢' },
    { id: '24', name: '饥饿' },
    { id: '25', name: '困' },
    { id: '26', name: '惊恐' },
    { id: '27', name: '流汗' },
    { id: '28', name: '憨笑' },
    { id: '29', name: '悠闲' },
    { id: '30', name: '奋斗' },
    { id: '31', name: '咒骂' },
    { id: '32', name: '疑问' },
    { id: '33', name: '嘘' },
    { id: '34', name: '晕' },
    { id: '35', name: '折磨' },
    { id: '36', name: '衰' },
    { id: '37', name: '骷髅' },
    { id: '38', name: '敲打' },
    { id: '39', name: '再见' },
    { id: '41', name: '发抖' },
    { id: '42', name: '爱情' },
    { id: '43', name: '跳跳' },
    { id: '46', name: '猪头' },
    { id: '49', name: '拥抱' },
    { id: '53', name: '蛋糕' },
    { id: '55', name: '炸弹' },
    { id: '56', name: '刀' },
    { id: '59', name: '便便' },
    { id: '60', name: '咖啡' },
    { id: '63', name: '玫瑰' },
    { id: '64', name: '凋谢' },
    { id: '66', name: '爱心' },
    { id: '67', name: '心碎' },
    { id: '74', name: '太阳' },
    { id: '75', name: '月亮' },
    { id: '76', name: '赞' },
    { id: '77', name: '踩' },
    { id: '78', name: '握手' },
    { id: '79', name: '胜利' },
    { id: '85', name: '飞吻' },
    { id: '86', name: '怄火' },
    { id: '89', name: '西瓜' },
    { id: '96', name: '冷汗' },
    { id: '97', name: '擦汗' },
    { id: '98', name: '抠鼻' },
    { id: '99', name: '鼓掌' },
    { id: '100', name: '糗大了' },
    { id: '101', name: '坏笑' },
    { id: '102', name: '左哼哼' },
    { id: '103', name: '右哼哼' },
    { id: '104', name: '哈欠' },
    { id: '105', name: '鄙视' },
    { id: '106', name: '委屈' },
    { id: '107', name: '快哭了' },
    { id: '108', name: '阴险' },
    { id: '109', name: '左亲亲' },
    { id: '110', name: '吓' },
    { id: '111', name: '可怜' },
    { id: '112', name: '菜刀' },
    { id: '114', name: '篮球' },
    { id: '116', name: '示爱' },
    { id: '118', name: '抱拳' },
    { id: '119', name: '勾引' },
    { id: '120', name: '拳头' },
    { id: '121', name: '差劲' },
    { id: '123', name: 'NO' },
    { id: '124', name: 'OK' },
    { id: '125', name: '转圈' },
    { id: '129', name: '挥手' },
    { id: '137', name: '鞭炮' },
    { id: '144', name: '喝彩' },
    { id: '146', name: '爆筋' },
    { id: '147', name: '棒棒糖' },
    { id: '148', name: '喝奶' },
    { id: '169', name: '手枪' },
    { id: '171', name: '茶' },
    { id: '172', name: '眨眼睛' },
    { id: '173', name: '泪奔' },
    { id: '174', name: '无奈' },
    { id: '175', name: '卖萌' },
    { id: '176', name: '小纠结' },
    { id: '177', name: '喷血' },
    { id: '178', name: '斜眼笑' },
    { id: '179', name: 'doge' },
    { id: '181', name: '戳一戳' },
    { id: '182', name: '笑哭' },
    { id: '183', name: '我最美' },
    { id: '185', name: '羊驼' },
    { id: '187', name: '幽灵' },
    { id: '201', name: '点赞' },
    { id: '212', name: '托腮' },
    { id: '262', name: '脑阔疼' },
    { id: '263', name: '沧桑' },
    { id: '264', name: '捂脸' },
    { id: '265', name: '辣眼睛' },
    { id: '266', name: '哦哟' },
    { id: '267', name: '头秃' },
    { id: '268', name: '问号脸' },
    { id: '269', name: '暗中观察' },
    { id: '270', name: 'emm' },
    { id: '271', name: '吃瓜' },
    { id: '272', name: '呵呵哒' },
    { id: '273', name: '我酸了' },
    { id: '277', name: '汪汪' },
    { id: '281', name: '无眼笑' },
    { id: '282', name: '敬礼' },
    { id: '283', name: '狂笑' },
    { id: '284', name: '面无表情' },
    { id: '285', name: '摸鱼' },
    { id: '286', name: '魔鬼笑' },
    { id: '287', name: '哦' },
    { id: '289', name: '睁眼' },
    { id: '293', name: '摸锦鲤' },
    { id: '294', name: '期待' },
    { id: '295', name: '拿到红包' },
    { id: '297', name: '拜谢' },
    { id: '298', name: '元宝' },
    { id: '299', name: '牛啊' },
    { id: '300', name: '胖三斤' },
    { id: '302', name: '左拜年' },
    { id: '303', name: '右拜年' },
    { id: '305', name: '右亲亲' },
    { id: '306', name: '牛气冲天' },
    { id: '307', name: '喵喵' },
    { id: '311', name: '打call' },
    { id: '312', name: '变形' },
    { id: '314', name: '仔细分析' },
    { id: '317', name: '菜汪' },
    { id: '318', name: '崇拜' },
    { id: '319', name: '比心' },
    { id: '320', name: '庆祝' },
    { id: '323', name: '嫌弃' },
    { id: '324', name: '吃糖' },
    { id: '325', name: '惊吓' },
    { id: '326', name: '生气' },
    { id: '332', name: '举牌牌' },
    { id: '333', name: '烟花' },
    { id: '334', name: '虎虎生威' },
    { id: '336', name: '豹富' },
    { id: '337', name: '花朵脸' },
    { id: '338', name: '我想开了' },
    { id: '339', name: '舔屏' },
    { id: '341', name: '打招呼' },
    { id: '342', name: '酸Q' },
    { id: '343', name: '我方了' },
    { id: '344', name: '大怨种' },
    { id: '345', name: '红包多多' },
    { id: '346', name: '你真棒棒' },
    { id: '347', name: '大展宏兔' },
    { id: '349', name: '坚强' },
    { id: '350', name: '贴贴' },
    { id: '351', name: '敲敲' },
    { id: '352', name: '咦' },
    { id: '353', name: '拜托' },
    { id: '354', name: '尊嘟假嘟' },
    { id: '355', name: '耶' },
    { id: '356', name: '666' },
    { id: '357', name: '裂开' },
    { id: '358', name: '骰子' },
    { id: '359', name: '包剪锤' },
    { id: '360', name: '亲亲' },
    { id: '361', name: '狗狗笑哭' },
    { id: '362', name: '好兄弟' },
    { id: '363', name: '狗狗可怜' },
    { id: '364', name: '超级赞' },
    { id: '365', name: '狗狗生气' },
    { id: '366', name: '芒狗' },
    { id: '367', name: '狗狗疑问' },
    { id: '368', name: '奥特笑哭' },
    { id: '369', name: '彩虹' },
    { id: '370', name: '祝贺' },
    { id: '371', name: '冒泡' },
    { id: '372', name: '气呼呼' },
    { id: '373', name: '忙' },
    { id: '374', name: '波波流泪' },
    { id: '375', name: '超级鼓掌' },
    { id: '376', name: '跺脚' },
    { id: '377', name: '嗨' },
    { id: '378', name: '企鹅笑哭' },
    { id: '379', name: '企鹅流泪' },
    { id: '380', name: '真棒' },
    { id: '381', name: '路过' },
    { id: '382', name: 'emo' },
    { id: '383', name: '企鹅爱心' },
    { id: '384', name: '晚安' },
    { id: '385', name: '太气了' },
    { id: '386', name: '呜呜呜' },
    { id: '387', name: '太好笑' },
    { id: '388', name: '太头疼' },
    { id: '389', name: '太赞了' },
    { id: '390', name: '太头秃' },
    { id: '391', name: '太沧桑' },
    { id: '392', name: '龙年快乐' },
    { id: '393', name: '新年中龙' },
    { id: '394', name: '新年大龙' },
    { id: '395', name: '略略略' },
    { id: '396', name: '狼狗' },
    { id: '397', name: '抛媚眼' },
    { id: '398', name: '超级ok' },
    { id: '399', name: 'tui' },
    { id: '400', name: '快乐' },
    { id: '401', name: '超级转圈' },
    { id: '402', name: '别说话' },
    { id: '403', name: '出去玩' },
    { id: '404', name: '闪亮登场' },
    { id: '405', name: '好运来' },
    { id: '406', name: '姐是女王' },
    { id: '407', name: '我听听' },
    { id: '408', name: '臭美' },
    { id: '409', name: '送你花花' },
    { id: '410', name: '么么哒' },
    { id: '411', name: '一起嗨' },
    { id: '412', name: '开心' },
    { id: '413', name: '摇起来' },
    { id: '415', name: '划龙舟' },
    { id: '416', name: '中龙舟' },
    { id: '417', name: '大龙舟' },
    { id: '418', name: '填罐罐' },
    { id: '419', name: '火车' },
    { id: '420', name: '中火车' },
    { id: '421', name: '大火车' },
    { id: '422', name: '粽于等到你' },
    { id: '423', name: '复兴号' },
    { id: '424', name: '续标识' },
    { id: '425', name: '求放过' },
    { id: '426', name: '玩火' },
    { id: '427', name: '偷感' },
    { id: '428', name: '收到' },
    { id: '429', name: '蛇年快乐' },
    { id: '430', name: '蛇身' },
    { id: '431', name: '蛇尾' },
    { id: '432', name: '灵蛇献瑞' },
    { id: '433', name: '大鸽' },
    { id: '434', name: '鸽来了' },
    { id: '435', name: '关我鸟事' },
    { id: '436', name: '跟鸽走' },
    { id: '437', name: '鸽鸽生气' },
    { id: '438', name: '可靠' },
    { id: '439', name: '鸽瞅瞅' },
    { id: '440', name: '鸽喜欢' },
    { id: '441', name: '相信鸽' },
    { id: '442', name: '鸽王' },
    { id: '443', name: '戳一下' },
    { id: '444', name: '超级6' },
    { id: '445', name: '比个心' },
    { id: '446', name: '超级心碎' },
    { id: '447', name: '太赞了' },
    { id: '448', name: '放大招' },
    { id: '449', name: '+1' },
    { id: '450', name: '撇嘴' },
    { id: '451', name: '色' },
    { id: '452', name: '微笑' },
    { id: '453', name: '发呆' },
    { id: '454', name: '得意' },
    { id: '455', name: '害羞' },
    { id: '456', name: '闭嘴' },
    { id: '457', name: '睡' },
    { id: '458', name: '我吗' },
    { id: '459', name: '优雅' },
    { id: '460', name: '硬撑' },
    { id: '461', name: '宕机' },
    { id: '462', name: '无语' },
    { id: '463', name: '新年快乐' },
    { id: '464', name: '马上到' },
    { id: '465', name: '拆红包' },
    { id: '466', name: '羞羞哒' },
    { id: '467', name: '摇花手' },
    { id: '468', name: '失眠' },
    { id: '469', name: '坚毅' },
    { id: '470', name: '马到成功' },
    { id: '471', name: '深情' },
    { id: '472', name: '心动' },
    { id: '473', name: '爱意' },
    { id: '474', name: '给你一拳' },
    { id: '475', name: '干饭' },
    { id: '476', name: '不是吧' },
    { id: '477', name: '你懂的' },
    { id: '478', name: '对的对的' },
    { id: '479', name: '不对不对' },
    { id: '480', name: '散味儿' },
    { id: '481', name: '学习' },
    { id: '482', name: '热化了' },
    { id: '483', name: '略' },
    { id: '484', name: '比爱心' },
    { id: '485', name: '开学啦' },
    { id: '486', name: '开学啦2' },
    { id: '487', name: '开学啦3' },
    { id: '488', name: '开学大吉' },
    { id: '489', name: '知识增加' },
    { id: '490', name: '能送我吗' },
    { id: '491', name: '自信学霸' },
    { id: '492', name: '让我细品' },
    { id: '493', name: '又是早八' },
    { id: '494', name: '举杯邀月' },
    { id: '495', name: '兔来' },
    { id: '496', name: '阴晴圆缺' },
    { id: '497', name: '休假了' },
    { id: '498', name: '中!' },
    { id: '499', name: '观月C位' },
    { id: '500', name: '秋秋赏月' },
    { id: '501', name: '秋秋之约' },
    { id: '502', name: '秋秋泛舟' },
    { id: '503', name: '秋愿达成' },
    { id: '504', name: '同你玩' },
    { id: '505', name: '送花花' },
    { id: '506', name: '捡到宝了' },
    { id: '507', name: '被发现了' },
];
