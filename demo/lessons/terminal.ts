import type { DemoDriver, DemoLesson } from '../lesson-types';
import { terminalStore } from '@desktop/hooks/terminal/terminalStore';
import { terminalLayout, terminalPrefs } from '@desktop/hooks/terminal/terminalPrefs';
import { terminalService } from '@desktop/core/services/terminal.service';
import { getRuntime } from '@desktop/modules/terminal/registry';
import { resetWorkspaceDemo } from '../workspace.mock';

const activeSession = () => {
  const state = terminalStore.getSnapshot();
  return state.groups.find(group => group.id === state.activeGroup)?.focused;
};

async function command(d: DemoDriver, value: string) {
  await d.until(() => !!activeSession() && !!getRuntime(activeSession()!));
  const id = activeSession()!;
  await d.point(d.by('.xterm-screen'));
  for (const character of value) {
    await d.wait(90);
    getRuntime(id)!.fillInput(character);
  }
  await d.wait(450);
  await terminalService.write(id, '\r');
  await d.wait(1300);
}

export const terminalLesson: DemoLesson = {
  id: 'terminal', route: 'bots',
  steps: [
    ['打开本机终端', '终端使用 Desktop 的真实界面；命令在浏览器里模拟执行。'],
    ['查看当前目录', '输入 pwd，再按回车查看当前目录。'],
    ['输入一条命令', 'echo 打印一行文字，可以直接在这里继续输入。'],
    ['打开文件栏', '在终端旁浏览演示目录，不会访问电脑上的文件。'],
    ['左右分屏', '同一标签里打开两个独立会话。'],
    ['切换分屏方向', '上下或左右排列，按操作习惯调整。'],
  ],
  reset() {
    resetWorkspaceDemo('terminal');
    for (const id of Object.keys(terminalStore.getSnapshot().sessions)) void terminalStore.close(id);
    terminalLayout.patch({ height: 510, filesOpen: false });
    terminalPrefs.patch({ gpu: false });
    terminalStore.setOpen(true);
    terminalStore.setMaximized(false);
  },
  actions: [
    async d => {
      await d.click(d.button('开一个本机终端'));
      await d.until(() => !!activeSession() && !!getRuntime(activeSession()!));
      await d.wait(900);
    },
    async d => { await command(d, 'pwd'); },
    async d => { await command(d, 'echo Hello, NapCat!'); },
    async d => {
      await d.click(d.by('button[aria-label="文件栏"]'));
      await d.find(d.text('Documents', 'button,span'));
      await d.wait(1400);
    },
    async d => {
      await d.click(d.by('button[aria-label="收起文件栏"]'));
      await d.click(d.by('button[aria-label="左右分屏"]'));
      await command(d, 'echo Second session');
    },
    async d => {
      await d.click(d.by('button[aria-label="改成上下分屏"]'));
      await d.wait(1500);
      await d.click(d.by('button[aria-label="改成左右分屏"]'));
      await d.wait(1200);
    },
  ],
  observe() {
    const state = terminalStore.getSnapshot();
    const group = state.groups.find(item => item.id === state.activeGroup);
    if (group?.panes.length === 2) return group.split === 'column' ? 5 : 4;
    const session = activeSession();
    if (session && state.sessions[session]?.filesOpen) return 3;
    if (session && getRuntime(session)?.bufferText().includes('Hello, NapCat!')) return 2;
    return session ? 1 : 0;
  },
};
