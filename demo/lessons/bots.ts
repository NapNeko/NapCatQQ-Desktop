import type { DemoLesson, DemoTarget } from '../lesson-types';
import { demoReadConfig, mockSnapshots, seedDemoBot } from '../bot.mock';

const botId = '10003';
const visible = (element: HTMLElement) => element.getClientRects().length > 0;
const has = (selector: string) => [...document.querySelectorAll<HTMLElement>(selector)].some(visible);
const configOpen = () => has('[data-tour-id="bot-config-header"]');
const connectionsOpen = () => has('[data-tour-id="bot-connections-body"]');
const logOpen = () => has('[role="log"]');

function input(label: string): HTMLInputElement | null {
  const direct = [...document.querySelectorAll<HTMLInputElement>('input[aria-label]')]
    .find(element => element.getAttribute('aria-label') === label && visible(element));
  if (direct) return direct;
  const fieldLabel = [...document.querySelectorAll<HTMLLabelElement>('label')]
    .find(element => element.textContent?.replaceAll('*', '').trim() === label && visible(element));
  const field = fieldLabel?.htmlFor ? document.getElementById(fieldLabel.htmlFor) : null;
  return field instanceof HTMLInputElement ? field : null;
}

function switchFor(label: string): DemoTarget {
  return () => {
    const fieldLabel = [...document.querySelectorAll<HTMLLabelElement>('label')]
      .find(element => element.textContent?.includes(label) && visible(element));
    return fieldLabel?.querySelector<HTMLElement>('[role="switch"]') ?? null;
  };
}

function level(label: string): DemoTarget {
  return () => [...document.querySelectorAll<HTMLButtonElement>('[aria-label="日志级别筛选"] [role="radio"]')]
    .find(element => element.textContent?.trim() === label && visible(element)) ?? null;
}

const logSearch = () => input('搜索日志关键字');
const logPaused = () => has('button[title="已暂停自动滚动"]');
const dialogOpen = () => has('[role="dialog"]');
const connectionRow = () => [...document.querySelectorAll<HTMLElement>('[data-tour-id="bot-connections-body"] span')]
  .find(element => element.textContent?.trim() === 'local-http' && visible(element)) ?? null;

const connectionsLesson: DemoLesson = {
  id: 'connections',
  route: 'bots',
  reset: () => seedDemoBot(),
  steps: [
    ['打开 Bot 配置', '选中本机 NapCat 的「配置参数」。这里已有一份演示配置，还没有协议连接。'],
    ['切到连接页', 'OneBot 连接供应用端收发 API 与事件，与登录 QQ 用的 WebUI 是两回事。'],
    ['新增 HTTP 服务器', '在底部工具条选择「HTTP 服务器」，让应用程序向 Bot 发起 HTTP 请求。'],
    ['填写本机监听与鉴权', '连接名填 local-http，监听 127.0.0.1:3000。这里使用示例 Token；实际使用时保留随机值并妥善保存。'],
    ['先保存这一条连接', '「保存连接」只把连接放回表单，整份 Bot 配置还没有保存。'],
    ['再保存 Bot 配置', '确认「未保存」消失。演示不会监听端口或发送 HTTP 请求；实际配置后还需按正文查询登录信息。'],
  ],
  actions: [
    async driver => {
      await driver.click(driver.by('button[aria-label="配置参数"]'));
      await driver.find(driver.by('[data-tour-id="bot-config-header"]'));
      await driver.wait(600);
    },
    async driver => {
      await driver.click(driver.by('[data-tour-id="bot-connections-tab"]'));
      await driver.find(driver.by('[data-tour-id="bot-connections-body"]'));
      await driver.wait(700);
    },
    async driver => {
      await driver.click(driver.button('HTTP 服务器'));
      await driver.find(() => input('连接名称'));
      await driver.wait(600);
    },
    async driver => {
      await driver.type(() => input('连接名称'), 'local-http');
      await driver.type(() => input('监听 IP (Host)'), '127.0.0.1');
      await driver.type(() => input('端口'), '3000');
      await driver.type(() => input('鉴权 Token'), 'demo-token-10003');
      await driver.point(() => input('鉴权 Token'));
      await driver.wait(700);
    },
    async driver => {
      await driver.click(driver.button('保存连接'));
      await driver.until(() => !dialogOpen(), '连接编辑窗口没有关闭');
      await driver.point(connectionRow);
      await driver.wait(850);
    },
    async driver => {
      await driver.click(driver.button('保存'));
      await driver.until(() => demoReadConfig(botId)?.connect.httpServers.some(item =>
        item.name === 'local-http' && item.host === '127.0.0.1' && item.port === 3000
        && item.token === 'demo-token-10003') === true, 'HTTP 连接没有保存到 Bot 配置');
      await driver.point(driver.by('[data-tour-id="bot-save-actions"]'));
      await driver.wait(900);
    },
  ],
  observe() {
    if (demoReadConfig(botId)?.connect.httpServers.length) return 5;
    if (connectionRow()) return 4;
    if (dialogOpen() && input('连接名称')) return 3;
    if (connectionsOpen()) return 2;
    if (configOpen()) return 1;
    return 0;
  },
};

const botsLesson: DemoLesson = {
  id: 'bots',
  route: 'bots',
  reset: () => seedDemoBot(botId, '我的日常 Bot', 'running'),
  steps: [
    ['核对实例与运行状态', '这是一份本机 NapCat 演示配置。「运行中」只表示进程运行，不代表 QQ 已登录。'],
    ['先停止 Bot', '点击「停止 Bot」，等状态稳定后再修改重启设置。这里模拟停止，不会操作电脑上的 QQ。'],
    ['打开身份配置', '从「配置参数」进入身份页，QQ 账号在编辑时锁定，名称和自愈设置仍可修改。'],
    ['开启掉线自动重启', '把名称改为「日常值守 Bot」，开启「掉线自动重启」。它不能代替 QQ 的扫码和安全验证。'],
    ['保存修改', '保存后返回列表，检查名称是否更新。演示只保存本页内存，不会创建后台值守任务。'],
    ['重新启动并观察状态', '等待进程从启动中变成运行中。真实使用时，还要确认 QQ 登录与 OneBot 连接正常。'],
  ],
  actions: [
    async driver => {
      await driver.point(driver.by('[data-tour-id="bot-list-header"]'));
      await driver.wait(800);
    },
    async driver => {
      await driver.click(driver.by('button[aria-label="停止 Bot"]'));
      await driver.until(() => mockSnapshots.find(bot => bot.bot_id === botId)?.state === 'stopped', 'Bot 没有停止');
      await driver.wait(600);
    },
    async driver => {
      await driver.click(driver.by('button[aria-label="配置参数"]'));
      await driver.find(() => input('实例名称'));
      await driver.wait(600);
    },
    async driver => {
      await driver.type(() => input('实例名称'), '日常值守 Bot');
      await driver.click(switchFor('掉线自动重启'));
      await driver.until(() => switchFor('掉线自动重启')()?.getAttribute('aria-checked') === 'true', '重启开关没有开启');
      await driver.wait(800);
    },
    async driver => {
      await driver.click(driver.button('保存'));
      await driver.until(() => demoReadConfig(botId)?.bot.offlineAutoRestart === true, '重启设置没有保存');
      await driver.wait(500);
      await driver.click(driver.by('button[aria-label="返回列表"]'));
      await driver.find(driver.by('button[aria-label="启动 Bot"]'));
      await driver.wait(700);
    },
    async driver => {
      await driver.click(driver.by('button[aria-label="启动 Bot"]'));
      await driver.until(() => mockSnapshots.find(bot => bot.bot_id === botId)?.state === 'running', 'Bot 没有进入运行状态');
      await driver.point(driver.by('button[aria-label="查看日志"]'));
      await driver.wait(900);
    },
  ],
  observe() {
    const config = demoReadConfig(botId);
    const state = mockSnapshots.find(bot => bot.bot_id === botId)?.state;
    if (config?.bot.offlineAutoRestart) return state === 'running' || state === 'starting' ? 5 : 4;
    if (switchFor('掉线自动重启')()?.getAttribute('aria-checked') === 'true') return 3;
    if (configOpen()) return 2;
    if (state === 'stopped' || state === 'stopping') return 1;
    return 0;
  },
};

const failureLogs = [
  '10-20 10:00:00 [INFO] [演示] 以下是用于练习筛选的示例日志，不是当前电脑的运行记录。',
  '10-20 10:00:01 [INFO] [演示] NapCat 进程已启动，等待 QQ 登录。',
  '10-20 10:00:02 [DEBUG] [演示] 检查 OneBot 连接 local-http 的鉴权配置。',
  '10-20 10:00:03 [WARN] [演示] OneBot 请求返回 401，请核对两端 Token。',
  '10-20 10:00:04 [ERROR] [演示] OneBot 连接失败：鉴权 Token 不一致。',
  '10-20 10:00:05 [INFO] [演示] 先保留错误和上下文，再修改配置重试。',
];

const logsLesson: DemoLesson = {
  id: 'tasks-logs',
  route: 'bots',
  reset: () => seedDemoBot(botId, '日志排查示例', 'running', failureLogs),
  steps: [
    ['打开对应 Bot 的日志', 'Bot 启动与连接问题看实例日志；下载和安装失败应看「任务」。这次使用一份模拟鉴权失败记录。'],
    ['先看错误前后的记录', '时间、实例和上下文一起看，先找第一条明确错误，不要只截最后一行。'],
    ['搜索 OneBot', '用关键词缩小范围，找出与连接、鉴权相关的日志。搜索不会修改配置。'],
    ['只看错误级别', '选择「错误」，定位 Token 不一致的示例错误。筛选后的结果还需要结合上下文判断。'],
    ['暂停自动滚动', '暂停滚动后再仔细阅读。这里只暂停视图，不会停止真实 Bot 或后台任务。'],
    ['恢复完整上下文', '清空关键词并切回「全部」，把错误与前后记录一起保留。分享日志前隐藏真实 Token 和私人消息。'],
  ],
  actions: [
    async driver => {
      await driver.click(driver.by('button[aria-label="查看日志"]'));
      await driver.find(driver.by('[role="log"]'));
      await driver.wait(600);
    },
    async driver => {
      await driver.until(() => document.querySelector('[role="log"]')?.textContent?.includes('Token 不一致') === true, '示例日志没有加载');
      await driver.point(driver.by('[role="log"]'));
      await driver.wait(1000);
    },
    async driver => {
      await driver.type(logSearch, 'OneBot');
      await driver.wait(800);
    },
    async driver => {
      await driver.click(level('错误'));
      await driver.until(() => level('错误')()?.getAttribute('aria-checked') === 'true', '错误筛选没有生效');
      await driver.wait(800);
    },
    async driver => {
      await driver.click(driver.by('button[title="已启用自动滚动"]'));
      await driver.until(logPaused, '自动滚动没有暂停');
      await driver.point(driver.by('[role="log"]'));
      await driver.wait(850);
    },
    async driver => {
      await driver.type(logSearch, '');
      await driver.click(level('全部'));
      await driver.until(() => level('全部')()?.getAttribute('aria-checked') === 'true'
        && document.querySelector('[role="log"]')?.textContent?.includes('先保留错误') === true, '日志上下文没有恢复');
      await driver.point(driver.by('[role="log"]'));
      await driver.wait(900);
    },
  ],
  observe() {
    if (!logOpen()) return 0;
    const query = logSearch()?.value ?? '';
    if (logPaused()) return !query && level('全部')()?.getAttribute('aria-checked') === 'true' ? 5 : 4;
    if (level('错误')()?.getAttribute('aria-checked') === 'true') return 3;
    if (query) return 2;
    return 1;
  },
};

export const botLessons: DemoLesson[] = [connectionsLesson, botsLesson, logsLesson];
