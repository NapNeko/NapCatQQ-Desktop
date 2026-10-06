// 浏览器预览：麦麦认识的人。语义照后端 resources/person：分页、搜称呼 / 昵称 / 账号、认识与否；
// 改称呼和理由、删（单个 / 批量）。

import { makeAppConfigError } from '../../domain/apps/appConfigError';
import type {
    AppInstance,
    MaiBotPerson,
    MaiBotPersonAction,
    MaiBotPersonOverview,
    MaiBotPersonPage,
    MaiBotPersonQuery,
    MaiBotResourceDone,
} from '../types';
import { withMockDelay } from './bootstrap.mock';

// 称呼、昵称、为什么这么叫、账号；称呼空着表示麦麦还没给 TA 起名
const PEOPLE: [string, string, string, string][] = [
    ['小林', '林林子', '群里大家都这么叫她', '2854196310'],
    ['阿杰', 'Jay_', '他自我介绍说叫阿杰', '1143259876'],
    ['猫猫头', '喵喵喵', '头像是一只橘猫', '3021457788'],
    ['班长', '王同学', '他在群里管签到', '2210938471'],
    ['老张', '张三丰', '年纪比大家大一点', '1520983346'],
    ['', '一只鸽子', '', '3399120475'],
    ['星星', '⭐星星⭐', '昵称里全是星星', '2745100923'],
    ['大佬', 'dalao', '什么问题都能答上来', '1098276635'],
    ['', 'user_7788', '', '3145099821'],
    ['阿柴', '柴犬本犬', '头像是柴犬', '2981033476'],
    ['学姐', '温柔学姐', '她说自己大三', '1672034598'],
    ['小鹿', 'Lu', '名字里有鹿', '2039948812'],
    ['', 'nobody', '', '3301229984'],
    ['摸鱼王', '今天也在摸鱼', '整天在群里摸鱼', '1788203345'],
    ['阿紫', '紫色星云', '喜欢紫色', '2567710983'],
    ['夜猫子', '不睡觉', '总是半夜出来聊天', '1934408276'],
    ['小周', '周周', '姓周', '2476019835'],
    ['', '路过的旅人', '', '3087721456'],
    ['团长', '副本团长', '每周组织打副本', '1409937265'],
    ['豆豆', '豆沙包', '她让大家叫她豆豆', '2295801347'],
    ['老师', '数学老师', '他是真的老师', '1376609982'],
    ['阿楠', '楠楠', '名字最后一个字', '2689014537'],
];

const GROUPS = ['麦麦测试群', '原神交流群', '摸鱼小分队'];

type Store = { people: MaiBotPerson[] };
const stores = new Map<string, Store>();

function store(inst: AppInstance): Store {
    if (inst.state !== 'running') throw makeAppConfigError('not_running', '启动麦麦后才能用');
    let s = stores.get(inst.id);
    if (!s) {
        const now = Date.now() / 1000;
        const people = PEOPLE.map(([name, nickname, reason, uid], i): MaiBotPerson => ({
            person_id: `p${(0x1a2b + i * 97).toString(16).padStart(8, '0')}`,
            name,
            name_reason: reason,
            platform: 'qq',
            // 开头补 0：真 QQ 号不会以 0 开头，预览里不会去拉陌生人的头像
            user_id: `0${uid}`,
            nickname,
            group_cards: GROUPS.slice(0, i % 3).map((g, j) => ({
                group_id: `${123450 + j}`,
                card: `${name || nickname}@${g.slice(0, 2)}`,
            })),
            is_known: !!name,
            first_seen: now - 86400 * (40 - i),
            last_seen: now - 3600 * (i * 5 + 1),
        }));
        s = { people };
        stores.set(inst.id, s);
    }
    return s;
}

const done = (affected: number, message: string): Promise<MaiBotResourceDone> =>
    withMockDelay({ affected, message });

export const mockMaiBotPersons = {
    list(inst: AppInstance, q: MaiBotPersonQuery): Promise<MaiBotPersonPage> {
        const s = store(inst);
        const kw = q.search.trim().toLowerCase();
        const rows = s.people.filter(
            (p) =>
                (q.filter === 'all' || (q.filter === 'known') === p.is_known) &&
                (!kw || [p.name, p.nickname, p.user_id].some((v) => v.toLowerCase().includes(kw))),
        );
        const start = (Math.max(1, q.page) - 1) * q.page_size;
        return withMockDelay({ total: rows.length, items: rows.slice(start, start + q.page_size) });
    },

    overview(inst: AppInstance): Promise<MaiBotPersonOverview> {
        const s = store(inst);
        const known = s.people.filter((p) => p.is_known).length;
        return withMockDelay({ total: s.people.length, known, unknown: s.people.length - known });
    },

    action(inst: AppInstance, a: MaiBotPersonAction): Promise<MaiBotResourceDone> {
        const s = store(inst);
        switch (a.op) {
            case 'update':
                s.people = s.people.map((p) =>
                    p.person_id === a.person_id
                        ? {
                              ...p,
                              name: a.name.trim(),
                              name_reason: a.name_reason.trim(),
                              is_known: a.is_known,
                              last_seen: Date.now() / 1000,
                          }
                        : p,
                );
                return done(1, '改好了');
            case 'delete':
                s.people = s.people.filter((p) => !a.person_ids.includes(p.person_id));
                return done(
                    a.person_ids.length,
                    a.person_ids.length > 1 ? `删掉了 ${a.person_ids.length} 个人` : '删掉了',
                );
        }
    },
};
