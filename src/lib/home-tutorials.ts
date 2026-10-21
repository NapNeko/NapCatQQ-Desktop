import type { Tutorial } from './tutorials';

export const terminalTutorial: Tutorial = {
  id: 'terminal', title: '内嵌终端', poster: 'terminal',
  environment: '浏览器内模拟终端，命令和文件都来自演示数据。',
  steps: ['打开', '目录', '输出', '文件', '分屏', '切换'],
  summary: '输入命令、浏览文件，再把两个终端并排打开。',
};

export const appearanceTutorial: Tutorial = {
  id: 'appearance', title: '外观设置', poster: 'settings-appearance',
  environment: '设置仅作用于这个演示窗口。',
  steps: ['主题', '暗色', '应用', '侧栏', '浅色', '保存'],
  summary: '在真实设置页切换明暗主题，调整侧栏和圆角。',
};
