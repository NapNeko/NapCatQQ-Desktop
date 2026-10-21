import type { DemoLesson, DemoTarget } from '../lesson-types';
import { resetSystemDemo, systemDemoState } from '../system.mock';

const textOf = (element: Element) => element.textContent?.trim() ?? '';
const visible = (element: HTMLElement) => element.getClientRects().length > 0;
const exact = (selector: string, text: string): DemoTarget => () =>
  [...document.querySelectorAll<HTMLElement>(selector)]
    .find(element => textOf(element) === text && visible(element)) ?? null;
const tab = (text: string) => exact('button[role="tab"]', text);
const option = (text: string) => exact('[role="option"]', text);
const component = (id: string): DemoTarget => () =>
  document.querySelector<HTMLElement>(`[data-tour-id="comp-row-${id}"] article`);
const componentButton = (id: string, text: string): DemoTarget => () =>
  [...(component(id)()?.querySelectorAll<HTMLButtonElement>('button') ?? [])]
    .find(button => textOf(button) === text && !button.disabled) ?? null;
const fieldControl = (label: string, selector: string): DemoTarget => () => {
  const node = [...document.querySelectorAll<HTMLElement>('label')]
    .find(element => textOf(element) === label && visible(element));
  return node?.parentElement?.parentElement?.querySelector<HTMLElement>(selector) ?? null;
};
const radiusRound = () => fieldControl('圆角风格', 'button')()
  ?.parentElement?.querySelector<HTMLElement>('button:last-child') ?? null;
const debugSwitch = fieldControl('OneBot 调试台', '[role="switch"]');
const selectedTab = (name: string) => tab(name)()?.getAttribute('aria-selected') === 'true';
const savedButton = exact('button', '保存设置');
const isSaved = () => systemDemoState.savedRevision > 0
  && (savedButton() as HTMLButtonElement | null)?.disabled === true;
const qqReady = () => component('qq')()?.textContent?.includes('NCD 管理') ?? false;
const napcatReady = () => systemDemoState.isInstalled('napcat')
  && component('napcat')()?.textContent?.includes('已安装') === true;

const componentsLesson: DemoLesson = {
  id: 'components', route: 'components',
  steps: [
    ['选择本机', '先确认主机；组件安装在当前选中的机器上。'],
    ['查看缺失组件', '这份演示环境尚未安装 QQ 和 NapCat。'],
    ['先安装 QQ', '任务进度来自演示状态，不会下载或安装真实文件。'],
    ['检查运行库', 'VC++ 运行库已经准备好，无需重复安装。'],
    ['安装 NapCat', 'QQ 就绪后，安装协议端并等待任务完成。'],
    ['重新核对状态', '刷新后，QQ 显示 NCD 管理，NapCat 显示已安装。'],
  ],
  reset() { resetSystemDemo('components'); },
  actions: [
    async driver => { await driver.click(driver.by('[data-host-id="local"]')); },
    async driver => {
      await driver.find(componentButton('qq', '安装'));
      await driver.point(component('napcat'));
      await driver.point(component('qq'));
    },
    async driver => {
      await driver.click(componentButton('qq', '安装'));
      await driver.until(() => systemDemoState.isInstalled('qq') && qqReady(), 'QQ 安装状态未刷新');
      await driver.point(component('qq'));
    },
    async driver => { await driver.point(component('vcredist')); },
    async driver => {
      await driver.click(componentButton('napcat', '安装'));
      await driver.until(napcatReady, 'NapCat 安装状态未刷新');
      await driver.point(component('napcat'));
    },
    async driver => {
      await driver.click(driver.button('刷新'));
      await driver.until(() => qqReady() && napcatReady(), '组件探测尚未完成');
      await driver.point(component('qq'));
      await driver.point(component('napcat'));
    },
  ],
  observe() {
    if (systemDemoState.isInstalled('napcat') && systemDemoState.isInstalled('qq')) return 5;
    if (systemDemoState.isInstalled('qq')) return 4;
    if (component('qq')()?.textContent?.includes('演示')) return 2;
    return componentButton('qq', '安装')() ? 1 : 0;
  },
};

const settingsLesson: DemoLesson = {
  id: 'settings', route: 'settings',
  steps: [
    ['打开外观设置', '外观和功能开关共用一份设置草稿。'],
    ['选择圆润风格', '修改圆角后先留在草稿中，保存时才应用。'],
    ['切到功能页', '功能页控制侧栏与模块入口。'],
    ['启用调试台', '打开 OneBot 调试台，暂不离开设置页。'],
    ['保存修改', '保存后，界面偏好和功能开关一起生效。'],
    ['核对已保存外观', '返回外观页，圆润风格仍保持选中。'],
  ],
  reset() { resetSystemDemo('settings'); },
  actions: [
    async driver => {
      await driver.click(tab('外观'));
      await driver.point(fieldControl('圆角风格', 'button'));
    },
    async driver => {
      await driver.click(radiusRound);
      await driver.until(() => radiusRound()?.getAttribute('aria-pressed') === 'true');
    },
    async driver => { await driver.click(tab('功能')); },
    async driver => {
      const target = await driver.find(debugSwitch);
      if (target.getAttribute('aria-checked') !== 'true') await driver.click(debugSwitch);
      await driver.until(() => debugSwitch()?.getAttribute('aria-checked') === 'true');
    },
    async driver => {
      await driver.click(savedButton);
      await driver.until(isSaved, '设置尚未保存');
      await driver.point(savedButton);
    },
    async driver => {
      await driver.click(tab('外观'));
      await driver.until(() => radiusRound()?.getAttribute('aria-pressed') === 'true');
      await driver.point(radiusRound);
    },
  ],
  observe() {
    if (isSaved()) return 5;
    if (debugSwitch()?.getAttribute('aria-checked') === 'true') return 4;
    if (selectedTab('功能')) return 3;
    if (radiusRound()?.getAttribute('aria-pressed') === 'true') return 2;
    return 0;
  },
};

const proxyInput = (): HTMLInputElement | null =>
  document.querySelector<HTMLInputElement>('input[aria-label="代理地址"]');
const githubRoute = (): HTMLElement | null => document.getElementById('network-GitHub-route');
const simulatedResult = (): HTMLElement | null =>
  [...document.querySelectorAll<HTMLElement>('li')]
    .find(element => element.textContent?.includes('GitHub 下载')
      && element.textContent.includes('演示模拟通过')) ?? null;

const networkLesson: DemoLesson = {
  id: 'network', route: 'settings',
  steps: [
    ['进入网络页', '代理与下载线路控制 Desktop 发起的外网请求。'],
    ['填写手动代理', '使用示例地址 http://127.0.0.1:7890；演示不会连接它。'],
    ['选择下载线路', '把 GitHub 下载线路改为仅原站。'],
    ['模拟测试连接', '结果由演示生成，不代表当前电脑的网络可用性。'],
    ['保存网络设置', '连接测试只读草稿，保存后才应用设置。'],
    ['核对保存结果', '代理地址与 GitHub 线路保留在这份演示设置中。'],
  ],
  reset() { resetSystemDemo('network'); },
  actions: [
    async driver => {
      await driver.click(tab('网络'));
      await driver.point(driver.by('#network-proxy-mode'));
    },
    async driver => {
      await driver.select(driver.by('#network-proxy-mode'), option('手动配置'));
      await driver.type(driver.input('代理地址'), 'http://127.0.0.1:7890');
    },
    async driver => {
      await driver.select(githubRoute, option('仅原站'));
      await driver.until(() => githubRoute()?.getAttribute('title') === '仅原站');
    },
    async driver => {
      await driver.click(driver.button('测试连接'));
      await driver.until(() => systemDemoState.networkProbes > 0 && simulatedResult() !== null,
        '模拟连接测试尚未完成');
      await driver.point(simulatedResult);
    },
    async driver => {
      await driver.click(savedButton);
      await driver.until(isSaved, '网络设置尚未保存');
      await driver.point(savedButton);
    },
    async driver => {
      await driver.point(proxyInput);
      await driver.point(githubRoute);
    },
  ],
  observe() {
    if (isSaved()) return 5;
    if (systemDemoState.networkProbes > 0) return 4;
    if (githubRoute()?.getAttribute('title') === '仅原站') return 3;
    if (proxyInput()?.value === 'http://127.0.0.1:7890') return 2;
    return selectedTab('网络') ? 1 : 0;
  },
};

const cleanupResult = (): HTMLElement | null =>
  [...document.querySelectorAll<HTMLElement>('[role="status"]')]
    .find(element => element.textContent?.includes('已删除')) ?? null;
const checked = (id: string) => document.getElementById(id)?.getAttribute('data-state') === 'checked'
  || document.getElementById(id)?.getAttribute('aria-checked') === 'true';

const backupLesson: DemoLesson = {
  id: 'backup', route: 'settings',
  steps: [
    ['打开数据页', '配置备份与下载缓存都在设置的数据页。'],
    ['查看备份范围', 'ZIP 导入导出需桌面文件选择器；此处只查看范围。'],
    ['扫描演示缓存', '清理窗口使用真实界面，文件数量来自内存样例。'],
    ['只选择旧更新包', '取消安装包与下载残留，仅保留旧更新包。'],
    ['清理所选缓存', '模拟删除 1 个旧更新包，并保留近期文件。'],
    ['查看结果并关闭', '核对删除与跳过数量；不会删除电脑上的文件。'],
  ],
  reset() { resetSystemDemo('backup'); },
  actions: [
    async driver => { await driver.click(tab('数据')); },
    async driver => {
      await driver.point(driver.text('配置备份', 'h2'));
      await driver.point(fieldControl('导出当前配置', 'button'));
    },
    async driver => {
      await driver.click(driver.button('扫描与清理'));
      await driver.until(() => systemDemoState.cacheScans > 0
        && document.getElementById('cache-desktop_updates') !== null, '缓存扫描尚未完成');
      await driver.point(driver.by('#cache-desktop_updates'));
    },
    async driver => {
      if (checked('cache-installers')) await driver.click(driver.by('#cache-installers'));
      if (checked('cache-download_residues')) await driver.click(driver.by('#cache-download_residues'));
      if (!checked('cache-desktop_updates')) await driver.click(driver.by('#cache-desktop_updates'));
      await driver.point(driver.by('#cache-desktop_updates'));
    },
    async driver => {
      await driver.click(driver.button('清理所选缓存'));
      await driver.until(() => systemDemoState.cacheResult !== null && cleanupResult() !== null,
        '模拟缓存清理尚未完成');
      await driver.point(cleanupResult);
    },
    async driver => {
      await driver.point(cleanupResult);
      await driver.click(driver.button('查看未清理项目的原因'));
      await driver.point(driver.text('演示：最近 24 小时的更新包保留。', 'li'));
      await driver.click(exact('[role="dialog"] button', '关闭'));
    },
  ],
  observe() {
    if (systemDemoState.cacheResult !== null) return 5;
    if (systemDemoState.cacheScans > 0) {
      return checked('cache-desktop_updates') && !checked('cache-installers')
        && !checked('cache-download_residues') ? 4 : 3;
    }
    return selectedTab('数据') ? 1 : 0;
  },
};

const themePicker = fieldControl('主题', 'button');
const appearanceLesson: DemoLesson = {
  id: 'appearance', route: 'settings',
  steps: [
    ['打开主题面板', '明暗主题与配色都在外观页选择。'],
    ['选暗色与圆润风格', '先修改草稿，保存之后再应用到窗口。'],
    ['应用暗色主题', '整个工作区一起切换，设置页也会同步。'],
    ['换成悬浮侧栏', '侧栏可以在经典和悬浮两种形态间切换。'],
    ['再试试浅色', '选择浅色主题，保留刚刚调整的侧栏和圆角。'],
    ['保存外观', '现在可以接手操作，试试其他配色。'],
  ],
  reset() { resetSystemDemo('settings'); },
  actions: [
    async d => { await d.click(tab('外观')); await d.click(themePicker); await d.wait(700); },
    async d => {
      await d.click(d.by('button[title="暗色"]'));
      await d.click(themePicker);
      await d.click(radiusRound);
    },
    async d => { await d.click(savedButton); await d.until(isSaved); await d.wait(1700); },
    async d => { await d.click(exact('button', '悬浮')); },
    async d => {
      await d.click(themePicker);
      await d.click(d.by('button[title="浅色"]'));
      await d.click(themePicker);
    },
    async d => { await d.click(savedButton); await d.until(isSaved); await d.wait(1700); },
  ],
  observe() {
    if (systemDemoState.savedRevision > 1 && isSaved()) return 5;
    if (isSaved()) return 3;
    if (radiusRound()?.getAttribute('aria-pressed') === 'true') return 1;
    return 0;
  },
};

export const systemLessons: DemoLesson[] = [
  componentsLesson, settingsLesson, networkLesson, backupLesson, appearanceLesson,
];
