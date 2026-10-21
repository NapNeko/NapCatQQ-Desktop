import type { DemoLesson, DemoTarget } from '../lesson-types';
import { debugWorkspaceStore } from '@desktop/hooks/debug/debugWorkspaceStore';
import { instances } from '@desktop/core/ipc/mock/app-framework/state';
import {
  resetWorkspaceDemo, workspaceAppId, workspaceAppName, workspaceBotId, workspaceBotName,
} from '../workspace.mock';

const shown = (element: HTMLElement) => element.getClientRects().length > 0;
const has = (selector: string) => !!document.querySelector(selector);
const selectField = (name: string): DemoTarget => () => {
  const label = [...document.querySelectorAll('label')].find(element => element.textContent?.trim() === name);
  return label?.parentElement?.querySelector<HTMLElement>('[role="combobox"]') ?? null;
};
function debugResponse() {
  const active = debugWorkspaceStore.getSnapshot().ws.active_tab;
  return active ? debugWorkspaceStore.getRun(active)?.last : null;
}
const debugLesson: DemoLesson = {
  id: 'debugging', route: 'debug',
  steps: [
    ['选择目标 Bot', '演示 Bot 已登录。先核对顶栏账号，调用和事件通道保持自动。'],
    ['搜索登录信息接口', '搜索 get_login_info，打开这条只读请求。'],
    ['核对接口与参数', '先看文档和参数。get_login_info 不需要业务参数，保留空对象即可。'],
    ['发送并检查响应', '核对 status、retcode、data 和返回的账号。这里的响应由演示环境生成。'],
    ['打开调用历史', '历史保留当时的参数和回包；打开记录不会再次发送请求。'],
    ['收藏这次检查', '保存为「检查当前账号」，以后可以从收藏打开并核对参数。'],
  ],
  reset: () => resetWorkspaceDemo('debugging'),
  actions: [
    async d => {
      await d.select(d.by('button[aria-label^="当前 Bot："], button[aria-label="选择 Bot"]'), d.text(workspaceBotName, '[role="option"]'));
      await d.point(d.by('[aria-label="调试目标"]'));
      await d.wait(900);
    },
    async d => {
      await d.click(d.by('button[aria-label^="搜索接口（"]'));
      await d.type(d.by('input[aria-label="搜索接口"]'), 'get_login_info');
      await d.click(d.by('[role="option"][aria-label^="get_login_info，"]'));
      await d.find(d.by('[role="combobox"][aria-label="接口名"], button[aria-label="修改接口名"]'));
    },
    async d => {
      await d.point(d.by('[aria-label="请求与响应"]'));
      await d.find(d.by('button[title^="发送（"]'));
      await d.wait(1500);
    },
    async d => {
      await d.click(d.by('button[title^="发送（"]'));
      await d.until(() => !!debugResponse(), '未收到模拟接口响应');
      await d.point(d.by('[aria-label="请求与响应"]'));
      await d.wait(1700);
    },
    async d => {
      await d.click(d.text('历史', '[role="tablist"][aria-label="左栏面板"] [role="tab"]'));
      await d.click(d.text('get_login_info', '[data-debug-left-panel="history"] [role="option"]'));
      await d.wait(1200);
    },
    async d => {
      await d.click(d.by('button[aria-label="收藏这个请求"]'));
      await d.type(d.by('[role="dialog"] input[placeholder="get_login_info"]'), '检查当前账号');
      await d.click(d.text('收藏', '[role="dialog"] button[type="submit"]'));
      await d.click(d.text('收藏', '[role="tablist"][aria-label="左栏面板"] [role="tab"]'));
      await d.point(d.by('[role="treeitem"][aria-label^="检查当前账号"]'));
      await d.wait(1000);
    },
  ],
  observe: () => {
    if (has('[data-debug-left-panel="collections"]') || has('[role="dialog"] input[placeholder="get_login_info"]')) return 5;
    if (has('[data-debug-left-panel="history"]')) return 4;
    if (debugResponse()) return 3;
    if (debugWorkspaceStore.getSnapshot().ws.active_tab) return 2;
    if (has('input[aria-label="搜索接口"]')) return 1;
    return 0;
  },
};

const appsLesson: DemoLesson = {
  id: 'apps', route: 'apps',
  steps: [
    ['查看已准备的应用实例', '演示中已准备好本机 Karin，不执行下载或安装。先核对名称和运行状态。'],
    ['进入实例详情', '卡片进入真实设置页，实例各自保留主机、端口和配置。'],
    ['查看基础设置', '这里可以配置运行器、日志等项目。本次保持现有设置，继续建立 OneBot 连接。'],
    ['打开对接入口', '从「更多 → 对接」进入，同时核对协议 Bot 与应用实例。'],
    ['阅读对接计划', '本例两端都在本机。检查连接方向、地址和要修改的应用端文件。'],
    ['应用模拟对接', '对接只写入演示数据。实际使用时还要在两端日志确认连接并测试指令。'],
  ],
  reset: () => resetWorkspaceDemo('apps'),
  actions: [
    async d => { await d.point(d.text(workspaceAppName, 'article[role="button"]')); await d.wait(1000); },
    async d => {
      await d.click(d.text(workspaceAppName, 'article[role="button"]'));
      await d.find(d.text(workspaceAppName, 'h1'));
      await d.wait(700);
    },
    async d => {
      await d.click(d.button('基础'));
      await d.point(selectField('运行器'));
      await d.wait(1300);
    },
    async d => {
      await d.click(d.by('button[aria-label="更多"]'));
      await d.click(d.button('对接'));
      await d.find(d.text('对接应用端', '[role="dialog"] h2'));
    },
    async d => {
      await d.select(selectField('协议 Bot'), d.text(workspaceBotName, '[role="option"]'));
      await d.find(d.text('写入协议 Bot 的连接', '[role="dialog"] p'));
      await d.point(d.text('写入协议 Bot 的连接', '[role="dialog"] p'));
      await d.wait(1700);
    },
    async d => {
      await d.click(d.button('应用对接'));
      await d.until(() => instances.find(instance => instance.id === workspaceAppId)?.link?.bot_id === workspaceBotId, '模拟对接没有保存');
      await d.point(d.text(`已对接 ${workspaceBotId}`, 'header p'));
      await d.wait(1100);
    },
  ],
  observe: () => {
    if (instances.find(instance => instance.id === workspaceAppId)?.link?.bot_id === workspaceBotId) return 5;
    if ([...document.querySelectorAll('p')].some(element => element.textContent?.trim() === '写入协议 Bot 的连接')) return 4;
    if (has('[role="dialog"] [role="combobox"]')) return 3;
    if ([...document.querySelectorAll('h1')].some(element => element.textContent === workspaceAppName)) return 2;
    return 0;
  },
};

const overviewLesson: DemoLesson = {
  id: 'overview', route: 'overview',
  steps: [
    ['查看概览摘要', '演示中有一个运行实例和一个异常退出实例。进程数量不等于已登录的 QQ 数量。'],
    ['看运行数量与待处置', '实例状态统计协议 Bot。待处置旁边会列出需要检查的原因。'],
    ['查看最近通知', '先读通知的对象和详情，再进入对应页面处理；本页展示的都是演示数据。'],
    ['核对本机组件版本', 'NapCat 版本卡片对应运行 Desktop 的本机，不能代替远端服务器版本检查。'],
    ['进入异常 Bot', '从待处置项进入机器人列表，找到「待检查的 Bot」。'],
    ['先读运行日志', '预设日志说明了异常场景。实际排查先保留错误，再决定是否修改配置或重启。'],
  ],
  reset: () => resetWorkspaceDemo('overview'),
  actions: [
    async d => { await d.point(d.text('个实例运行中', 'button')); await d.wait(1300); },
    async d => {
      await d.point(d.by('[role="status"][aria-label^="待处置："]'));
      await d.point(d.by('[aria-labelledby="overview-actionable-heading"]'));
      await d.wait(1300);
    },
    async d => { await d.point(d.text('最近通知', 'h3')); await d.wait(1500); },
    async d => { await d.point(d.text('NapCat', 'p.font-display')); await d.wait(1300); },
    async d => {
      await d.click(d.text('待检查的 Bot', '[aria-labelledby="overview-actionable-heading"] button'));
      await d.point(d.by('[data-bot-card-id="10004"]'));
      await d.wait(800);
    },
    async d => {
      await d.click(d.by('[data-bot-card-id="10004"] button[aria-label="查看日志"]'));
      await d.point(d.text('运行日志', 'h2'));
      await d.wait(1600);
    },
  ],
  observe: () => {
    if ([...document.querySelectorAll('h2')].some(element => element.textContent?.includes('运行日志'))) return 5;
    if (has('[data-bot-card-id="10004"]')) return 4;
    return 0;
  },
};

const chatReply = '收到，我会先核对 Bot 日志。';
const replyButton: DemoTarget = () => [...document.querySelectorAll<HTMLElement>('[aria-label="消息记录"] button[aria-label^="回复"][aria-label$="的消息"]')].filter(shown).at(-1) ?? null;
const chatLesson: DemoLesson = {
  id: 'chat', route: 'chat',
  steps: [
    ['确认聊天账号', '聊天使用已登录的协议 Bot。本页账号、联系人和消息均为样例。'],
    ['打开样例会话', '从最近会话进入「小林」，核对窗口顶部的联系人。'],
    ['引用一条消息', '消息旁的回复入口会把引用加入输入框，发送前检查引用和接收人。'],
    ['发送演示回复', '使用真实输入框发送，页面只生成模拟回执，不会向 QQ 发出消息。'],
    ['搜索当前会话', '在已载入的聊天记录里搜索「收到」，找到刚才的回复。'],
    ['回到消息位置', '点击搜索结果定位上下文。实际搜索依赖本机保存的记录，不会拉取全部云端历史。'],
  ],
  reset: () => resetWorkspaceDemo('chat'),
  actions: [
    async d => { await d.point(d.by('button[aria-label="聊天账号"]')); await d.wait(1100); },
    async d => {
      await d.click(d.text('小林', '[role="listbox"][aria-label="会话"] [role="option"]'));
      await d.find(d.by('textarea[aria-label="发送消息给小林"]'));
      await d.wait(800);
    },
    async d => {
      await d.click(replyButton);
      await d.point(d.by('textarea[aria-label="发送消息给小林"]'));
      await d.wait(1000);
    },
    async d => {
      await d.type(d.by('textarea[aria-label="发送消息给小林"]'), chatReply);
      await d.click(d.by('button[aria-label="发送消息"]'));
      await d.find(d.text(chatReply, '[aria-label="消息记录"] p, [aria-label="消息记录"] span'));
      await d.wait(1000);
    },
    async d => {
      await d.click(d.by('[aria-label="当前会话"] button[aria-label="搜索当前会话"]'));
      const toggle = await d.find(d.by('button[aria-label="搜索消息"]'));
      if (toggle.getAttribute('aria-expanded') === 'false') await d.click(d.by('button[aria-label="搜索消息"]'));
      await d.type(d.by('input[placeholder="搜索当前会话"]'), '收到');
      await d.point(d.by('[aria-label="消息搜索结果"]'));
      await d.find(d.text(chatReply, '[aria-label="消息搜索结果"] [role="option"]'));
      await d.wait(1100);
    },
    async d => {
      await d.click(d.text(chatReply, '[aria-label="消息搜索结果"] [role="option"]'));
      await d.point(d.text(chatReply, '[aria-label="消息记录"] p, [aria-label="消息记录"] span'));
      await d.wait(1500);
    },
  ],
  observe: () => {
    if (has('[aria-label="消息搜索结果"]')) return 4;
    if ([...document.querySelectorAll('[aria-label="消息记录"] p, [aria-label="消息记录"] span')].some(element => element.textContent?.includes(chatReply))) return 3;
    const input = document.querySelector<HTMLTextAreaElement>('textarea[aria-label="发送消息给小林"]');
    if (input?.value || has('.native-chat-reply')) return 2;
    if (input) return 1;
    return 0;
  },
};

export const workspaceLessons: DemoLesson[] = [debugLesson, appsLesson, overviewLesson, chatLesson];
