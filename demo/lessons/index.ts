import { firstBotLesson } from './first-bot';
import { botLessons } from './bots';
import { systemLessons } from './system';
import { workspaceLessons } from './workspaces';

export const lessons = [firstBotLesson, ...botLessons, ...systemLessons, ...workspaceLessons];

export function getLesson(id: string | null) {
  return lessons.find(lesson => lesson.id === id) ?? firstBotLesson;
}
