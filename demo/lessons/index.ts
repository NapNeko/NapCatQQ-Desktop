import type { DemoLesson } from '../lesson-types';

type LessonEntry = {
  id: string;
  route: DemoLesson['route'];
  load: () => Promise<DemoLesson>;
};

function fromGroup(id: string, load: () => Promise<DemoLesson[]>): () => Promise<DemoLesson> {
  return async () => {
    const lesson = (await load()).find(item => item.id === id);
    if (!lesson) throw new Error(`未找到教程演示：${id}`);
    return lesson;
  };
}

const loadBots = () => import('./bots').then(module => module.botLessons);
const loadSystem = () => import('./system').then(module => module.systemLessons);
const loadWorkspaces = () => import('./workspaces').then(module => module.workspaceLessons);
const firstBot: LessonEntry = {
  id: 'first-bot', route: 'bots',
  load: () => import('./first-bot').then(module => module.firstBotLesson),
};
const entries: LessonEntry[] = [
  firstBot,
  ...['connections', 'bots', 'tasks-logs'].map(id => ({ id, route: 'bots' as const, load: fromGroup(id, loadBots) })),
  { id: 'components', route: 'components', load: fromGroup('components', loadSystem) },
  ...['settings', 'network', 'backup', 'appearance'].map(id => ({ id, route: 'settings' as const, load: fromGroup(id, loadSystem) })),
  { id: 'debugging', route: 'debug', load: fromGroup('debugging', loadWorkspaces) },
  { id: 'apps', route: 'apps', load: fromGroup('apps', loadWorkspaces) },
  { id: 'overview', route: 'overview', load: fromGroup('overview', loadWorkspaces) },
  { id: 'chat', route: 'chat', load: fromGroup('chat', loadWorkspaces) },
  { id: 'terminal', route: 'bots', load: () => import('./terminal').then(module => module.terminalLesson) },
];

export function getLessonEntry(id: string | null): LessonEntry {
  return entries.find(entry => entry.id === id) ?? firstBot;
}
