import { mockSnapshots } from '../bot.mock';
import type { DemoLesson } from '../lesson-types';

export const firstBotLesson: DemoLesson = {
  id: 'first-bot',
  route: 'bots',
  steps: [
    ['从机器人列表开始', 'QQ 与 NapCat 已准备好。点击「新增 Bot」，创建一份配置。'],
    ['打开新增 Bot', '这里是 Desktop 的真实配置页，按钮和表单都可以操作。'],
    ['填写账号和名称', '演示账号填 10003，实例名称填「我的第一个 Bot」，其余保持本机 NapCat 默认值。'],
    ['保存配置', '保存后返回列表。配置只保存在这次演示里。'],
    ['启动 Bot', '点击卡片上的启动按钮，观察进程从启动中变成运行中。'],
    ['查看运行日志', '进程运行不等于 QQ 已登录。实际使用时，接着打开 WebUI 完成扫码。'],
  ],
  reset() {},
  actions: [
    async d => { await d.wait(1000); },
    async d => {
      await d.click(d.by('[data-tour-id="bot-create-fab"], [data-tour-id="bot-create-empty"]'));
      await d.find(d.by('[placeholder="例如：10001"]'));
      await d.wait(650);
    },
    async d => {
      await d.type(d.by('[placeholder="例如：10001"]'), '10003');
      await d.type(d.input('实例名称'), '我的第一个 Bot');
      await d.point(d.by('[data-tour-id="bot-runtime-section"]'));
      await d.wait(1000);
    },
    async d => {
      await d.click(d.button('保存'));
      await d.until(() => mockSnapshots.length > 0, '配置没有保存成功，请重置后再试。');
      await d.wait(1000);
      await d.click(d.by('button[aria-label="返回列表"]'));
      await d.find(d.by('button[aria-label="启动 Bot"]'));
      await d.wait(900);
    },
    async d => {
      await d.click(d.by('button[aria-label="启动 Bot"]'));
      await d.until(() => mockSnapshots[0]?.state === 'running', 'Bot 没有进入运行状态，请查看提示或重置演示。');
      await d.wait(1000);
    },
    async d => {
      await d.click(d.by('button[aria-label="查看日志"]'));
      await d.find(d.text('运行日志', 'h2'));
      await d.wait(1800);
    },
  ],
  observe() {
    const current = mockSnapshots[0];
    let step = current ? current.state === 'running' || current.state === 'starting' ? 4 : 3 : 0;
    if (document.querySelector('[data-tour-id="bot-config-header"]') && !current) {
      step = document.querySelector<HTMLInputElement>('[placeholder="例如：10001"]')?.value ? 2 : 1;
    }
    if ([...document.querySelectorAll('h2')].some(element => element.textContent?.includes('运行日志'))) step = 5;
    return step;
  },
};
