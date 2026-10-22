import { describe, expect, it } from 'vitest';
import { BOT_SCHEMA, MODEL_SCHEMA, nodeAt } from '../../../../core/domain/apps/maibotSchema';
import { MAIBOT_TASK_KEYS } from '../../../../core/domain/apps/maibotConfig';
import { MAIBOT_SCHEMA_PAGES, maibotTabForIssue } from './maibotPages';

describe('MaiBot 配置快照的页面覆盖', () => {
    it('每个上游可视化主配置小节都有页面承接', () => {
        const roots = new Set(
            Object.values(MAIBOT_SCHEMA_PAGES).flatMap((page) =>
                page.sections.map((section) => section.path[0]),
            ),
        );
        expect(
            BOT_SCHEMA.fields.filter((field) => !roots.has(field.name)).map((field) => field.name),
        ).toEqual([]);
        expect(MODEL_SCHEMA.fields.map((field) => field.name).sort()).toEqual([
            'api_providers',
            'model_task_config',
            'models',
            'web',
        ]);
    });
    it('全部模型任务都在分配列表中，联网服务单独选择', () => {
        const tasks = nodeAt(MODEL_SCHEMA, ['model_task_config'])!.fields.map(
            (field) => field.name,
        );
        expect([...MAIBOT_TASK_KEYS, 'web_search'].sort()).toEqual(tasks.sort());
    });
    it('新能力的校验路径跳到可编辑的位置', () => {
        expect(maibotTabForIssue('bot/agent/acp_timeout')).toBe('agent');
        expect(maibotTabForIssue('bot/capabilities/planner_subagent')).toBe('agent');
        expect(maibotTabForIssue('models/web/services/search/timeout')).toBe('models');
    });
});
