import type { Tutorial } from '../lib/tutorials';
import { DemoResidency } from './demo-residency';
import { setThemeImage, type ThemeImageSources } from './theme-images';

let budgetNotification = 0;
let expandedDemo: HTMLElement | null = null;
const residency = new DemoResidency({
  now: () => performance.now(),
  setTimeout: (callback, delay) => window.setTimeout(callback, delay),
  clearTimeout: timer => window.clearTimeout(timer),
}, 2, 12_000, () => {
  if (budgetNotification) return;
  budgetNotification = window.setTimeout(() => {
    budgetNotification = 0;
    document.dispatchEvent(new Event('demo:budget-available'));
  }, 0);
});

const childModes = ['ready', 'playing', 'paused', 'manual', 'complete', 'error'] as const;
type ChildMode = (typeof childModes)[number];
type ViewMode = ChildMode | 'idle' | 'loading';
type Intent = 'play' | 'pause' | 'manual';
type Command = Intent | 'reset' | 'theme';
type DemoState = { mode: ChildMode; step: number; title: string; detail: string; error?: string };

function parseState(value: unknown, channel: string, stepCount: number): DemoState | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const data = value as Record<string, unknown>;
  if (data.channel !== channel || data.type !== 'state') return null;
  if (!childModes.includes(data.mode as ChildMode)) return null;
  if (typeof data.step !== 'number' || !Number.isInteger(data.step) || data.step < 0 || data.step >= stepCount) return null;
  if (typeof data.title !== 'string' || typeof data.detail !== 'string') return null;
  if (data.error !== undefined && typeof data.error !== 'string') return null;
  return {
    mode: data.mode as ChildMode,
    step: data.step,
    title: data.title.slice(0, 120),
    detail: data.detail.slice(0, 500),
    error: typeof data.error === 'string' ? data.error.slice(0, 500) : undefined,
  };
}

function setupDemo(root: HTMLElement) {
  let channel = `ncd-docs-demo:${root.dataset.demoLesson}`;
  const showcase = root.hasAttribute('data-demo-showcase');
  let frame = root.querySelector<HTMLIFrameElement>('[data-fbd-frame]')!;
  const viewport = root.querySelector<HTMLElement>('[data-fbd-viewport]')!;
  const toolbar = root.querySelector<HTMLElement>('.fbd-toolbar')!;
  const cover = root.querySelector<HTMLElement>('[data-fbd-cover]')!;
  const loading = root.querySelector<HTMLElement>('[data-fbd-loading]')!;
  const playButton = root.querySelector<HTMLButtonElement>('[data-fbd-play]')!;
  const playLabel = root.querySelector<HTMLElement>('[data-fbd-play-label]')!;
  const playIcon = root.querySelector<SVGElement>('[data-fbd-play-icon]')!;
  const pauseIcon = root.querySelector<SVGElement>('[data-fbd-pause-icon]')!;
  const manualButton = root.querySelector<HTMLButtonElement>('[data-fbd-manual]')!;
  const resetButton = root.querySelector<HTMLButtonElement>('[data-fbd-reset]')!;
  const expandButton = root.querySelector<HTMLButtonElement>('[data-fbd-expand]')!;
  const expandLabel = root.querySelector<HTMLElement>('[data-fbd-expand-label]')!;
  const progress = root.querySelector<HTMLDetailsElement>('[data-fbd-progress]')!;
  const progressSummary = progress.querySelector<HTMLElement>('summary')!;
  const stepPanel = progress.querySelector<HTMLElement>('.fbd-step-panel')!;
  const modeLabel = root.querySelector<HTMLElement>('[data-fbd-mode]')!;
  const position = root.querySelector<HTMLElement>('[data-fbd-position]')!;
  const title = root.querySelector<HTMLElement>('[data-fbd-title]')!;
  const detail = root.querySelector<HTMLElement>('[data-fbd-detail]')!;
  const errorNotice = root.querySelector<HTMLElement>('[data-fbd-error]')!;
  const panHint = root.querySelector<HTMLElement>('[data-fbd-pan]')!;
  let steps = Array.from(root.querySelectorAll<HTMLElement>('[data-fbd-step]'));
  const anchor = root.closest<HTMLElement>('[data-tutorial-anchor]')!;
  let source = root.dataset.demoSrc!;
  const choices = Array.from(root.querySelectorAll<HTMLButtonElement>('[data-fbd-choice]'));
  const demoWidth = Number(frame.width);
  const demoHeight = Number(frame.height);
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const compactScreen = window.matchMedia('(max-width: 760px)');
  const labels: Record<ViewMode, string> = {
    idle: '未开始', loading: '正在加载', ready: '已就绪', playing: '自动演示',
    paused: '已暂停', manual: '自己操作', complete: '演示完成', error: '演示出错',
  };
  let mode: ViewMode = 'idle';
  let mounted = false;
  let ready = false;
  let pendingIntent: Intent | null = null;
  let timeout: number | undefined;
  let expanded = false;
  let previousFocus: HTMLElement | null = null;
  let previousOverflow = '';
  let inertElements: Array<{ element: HTMLElement; wasInert: boolean }> = [];
  let inViewport = !showcase;
  let visibleRatio = showcase ? 0 : 1;
  let autoPaused = false;
  let autoStarted = false;
  let retainedByUser = false;
  let explicitlyStarted = false;
  const lease = residency.register(recycle);

  function syncResidency() {
    lease.update({ visible: (isVisible() && !document.hidden) || expanded, protected: retainedByUser || expanded || !showcase });
  }

  function isVisible() {
    if (expandedDemo && expandedDemo !== root) return false;
    return expanded || (explicitlyStarted ? visibleRatio > 0 : inViewport);
  }

  function replaceFrame() {
    const next = frame.cloneNode(false) as HTMLIFrameElement;
    next.removeAttribute('src');
    next.removeAttribute('srcdoc');
    frame.replaceWith(next);
    frame = next;
    next.addEventListener('error', () => {
      if (frame === next && mounted) showError('演示加载失败。请重试，或继续阅读图文步骤。');
    });
  }

  function recycle() {
    window.clearTimeout(timeout);
    mounted = ready = false;
    pendingIntent = null;
    autoPaused = autoStarted = false;
    explicitlyStarted = false;
    mode = 'idle';
    // A new browsing context rejects queued messages from the retired document.
    replaceFrame();
    setFrameAvailable(false);
    setText(position, `0 / ${steps.length}`);
    setText(title, '演示步骤');
    title.title = '';
    setText(detail, '演示中的操作只在演示环境内生效。');
    errorNotice.hidden = true;
    root.style.setProperty('--fbd-progress', '0');
    progressSummary.setAttribute('aria-label', '查看演示步骤与说明');
    steps.forEach(step => { step.removeAttribute('aria-current'); step.classList.remove('is-done'); });
    renderControls();
  }

  function setText(element: HTMLElement, text: string) {
    if (element.textContent !== text) element.textContent = text;
  }

  function post(command: Command) {
    const message: { channel: string; type: 'command'; command: Command; theme?: 'light' | 'dark' } = {
      channel, type: 'command', command,
    };
    if (command === 'theme') message.theme = document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
    frame.contentWindow?.postMessage(message, window.location.origin);
  }

  function renderControls() {
    root.dataset.mode = mode;
    setText(modeLabel, labels[mode]);
    const playing = mode === 'playing';
    setText(playLabel, mode === 'error' ? '重试加载' : playing ? '暂停'
      : mode === 'paused' ? '继续' : mode === 'complete' || mode === 'manual' ? '重新演示' : '自动演示');
    playIcon.toggleAttribute('hidden', playing);
    pauseIcon.toggleAttribute('hidden', !playing);
    playButton.disabled = false;
    manualButton.setAttribute('aria-pressed', String(mode === 'manual' || pendingIntent === 'manual'));
    resetButton.disabled = !ready;
    loading.hidden = mode !== 'loading';
    root.setAttribute('aria-busy', String(mode === 'loading'));
    if (showcase) setFrameAvailable(ready);
  }

  function updateStep(state: DemoState) {
    setText(position, `${state.step + 1} / ${steps.length}`);
    setText(title, state.title);
    title.title = state.title;
    setText(detail, state.detail);
    root.style.setProperty('--fbd-progress', String((state.step + 1) / steps.length));
    progressSummary.setAttribute('aria-label', `第 ${state.step + 1} 步，共 ${steps.length} 步：${state.title}，查看步骤与说明`);
    steps.forEach((step, index) => {
      step.classList.toggle('is-done', index < state.step || state.mode === 'complete');
      if (index === state.step) step.setAttribute('aria-current', 'step');
      else step.removeAttribute('aria-current');
    });
  }

  function setFrameAvailable(available: boolean) {
    cover.hidden = available;
    const interactive = available && (!showcase || mode === 'manual');
    if (interactive) frame.removeAttribute('aria-hidden');
    else frame.setAttribute('aria-hidden', 'true');
    frame.tabIndex = interactive ? 0 : -1;
    frame.inert = !interactive;
  }

  function showError(message: string) {
    window.clearTimeout(timeout);
    pendingIntent = null;
    mode = 'error';
    setText(errorNotice, message);
    errorNotice.hidden = false;
    renderControls();
  }

  function armTimeout() {
    window.clearTimeout(timeout);
    timeout = window.setTimeout(() => {
      if (!ready) showError('演示暂时无法加载。可以重试，或继续阅读下方图文步骤。');
    }, 30000);
  }

  function beginLoading(intent: Intent | null) {
    syncResidency();
    if (!lease.acquire(explicitlyStarted || retainedByUser || expanded)) return;
    replaceFrame();
    ready = false;
    pendingIntent = intent;
    mode = 'loading';
    setText(errorNotice, '');
    errorNotice.hidden = true;
    setFrameAvailable(false);
    setText(position, `0 / ${steps.length}`);
    setText(title, '正在加载');
    title.title = '';
    root.style.setProperty('--fbd-progress', '0');
    progressSummary.setAttribute('aria-label', '演示正在加载，查看步骤与说明');
    setText(detail, '演示中的操作只在演示环境内生效。');
    steps.forEach(step => { step.removeAttribute('aria-current'); step.classList.remove('is-done'); });
    renderControls();
    armTimeout();
    // A fresh document also clears private UI caches and timers in the reused frontend.
    mounted = true;
    const url = new URL(source, location.href);
    url.searchParams.set('theme', document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light');
    frame.src = url.href;
  }

  function request(intent: Intent) {
    if (!mounted || mode === 'error' || (intent === 'play' && (mode === 'manual' || mode === 'complete'))) {
      beginLoading(intent);
      return;
    }
    pendingIntent = intent;
    if (!ready) { renderControls(); return; }
    pendingIntent = null;
    if (intent !== 'pause' && !showcase) {
      viewport.scrollIntoView({
        block: expanded ? 'nearest' : 'center',
        inline: 'nearest',
        behavior: reducedMotion.matches ? 'instant' : 'smooth',
      });
    }
    post(intent);
    mode = intent === 'play' ? 'playing' : intent === 'pause' ? 'paused' : 'manual';
    renderControls();
  }

  function resize() {
    const width = viewport.clientWidth;
    if (!width) return;
    const gutter = window.innerWidth <= 560 ? 16 : 32;
    const availableHeight = Math.max(160, window.innerHeight - gutter - toolbar.offsetHeight - 2);
    // 放大窗口同时约束宽高；窄屏保留可操作尺寸，溢出由窗口内滚动承接。
    const scale = expanded
      ? Math.max(.65, Math.min(width / demoWidth, availableHeight / demoHeight))
      : Math.min(1, width / demoWidth);
    root.style.setProperty('--fbd-scale', String(scale));
    const overflow = demoWidth * scale > width + 1;
    panHint.hidden = !overflow && scale >= .6;
    setText(panHint, overflow ? '左右滑动查看窗口，或收起回到完整预览。' : '界面已缩小，点「放大」操作。');
    if (!choices.length) viewport.setAttribute('aria-label', overflow ? '软件演示窗口，可左右滚动' : '软件演示窗口');
    viewport.tabIndex = overflow ? 0 : -1;
  }

  function focusableElements() {
    return Array.from(root.querySelectorAll<HTMLElement>('button:not([disabled]), summary, a[href], input:not([disabled]), iframe, [tabindex]'))
      .filter(element => element.tabIndex >= 0 && element.getClientRects().length > 0 && !element.closest('[hidden], [inert]'));
  }

  function isolateExpandedView() {
    let branch: HTMLElement = root;
    inertElements = [];
    while (branch.parentElement && branch !== document.body) {
      for (const sibling of Array.from(branch.parentElement.children)) {
        if (sibling === branch || !(sibling instanceof HTMLElement) || sibling.matches('script, style, link')) continue;
        inertElements.push({ element: sibling, wasInert: sibling.inert });
        sibling.inert = true;
      }
      branch = branch.parentElement;
    }
  }

  function closeExpanded() {
    if (!expanded) return;
    expanded = false;
    if (expandedDemo === root) expandedDemo = null;
    root.dataset.expanded = 'false';
    root.setAttribute('role', 'region');
    root.removeAttribute('aria-modal');
    expandButton.setAttribute('aria-expanded', 'false');
    expandButton.title = '放大演示';
    setText(expandLabel, '放大');
    anchor.style.removeProperty('height');
    document.documentElement.style.overflow = previousOverflow;
    inertElements.forEach(({ element, wasInert }) => { element.inert = wasInert; });
    inertElements = [];
    if (showcase) {
      root.hidePopover();
      root.removeAttribute('popover');
    }
    resize();
    if (previousFocus?.isConnected) previousFocus.focus({ preventScroll: true });
    else expandButton.focus({ preventScroll: true });
    syncResidency();
    pauseWhenUnseen();
    document.dispatchEvent(new Event('demo:expanded-change'));
  }

  function openExpanded() {
    if (expanded) return;
    previousFocus = expandButton;
    anchor.style.height = `${anchor.getBoundingClientRect().height}px`;
    previousOverflow = document.documentElement.style.overflow;
    expanded = true;
    expandedDemo = root;
    root.dataset.expanded = 'true';
    root.setAttribute('role', 'dialog');
    root.setAttribute('aria-modal', 'true');
    expandButton.setAttribute('aria-expanded', 'true');
    expandButton.title = '收起演示';
    setText(expandLabel, '收起');
    document.documentElement.style.overflow = 'hidden';
    // Top layer 脱离透视与裁切，又不会因移动 iframe 而丢失操作状态。
    if (showcase) {
      root.setAttribute('popover', 'manual');
      root.showPopover();
    }
    isolateExpandedView();
    resize();
    expandButton.focus({ preventScroll: true });
    syncResidency();
    document.dispatchEvent(new Event('demo:expanded-change'));
  }

  window.addEventListener('message', (event: MessageEvent<unknown>) => {
    if (!mounted || event.source !== frame.contentWindow || event.origin !== window.location.origin) return;
    const message = event.data;
    if (message && typeof message === 'object' && 'channel' in message && message.channel === channel
      && 'type' in message && message.type === 'escape') {
      closeExpanded();
      return;
    }
    const state = parseState(message, channel, steps.length);
    if (!state || (state.mode === 'ready' && ready)) return;
    if (state.mode === 'error') {
      updateStep(state);
      showError(state.error || state.detail || '演示暂时无法继续，请重试。');
      return;
    }
    if (!ready && state.mode !== 'ready') return;
    if (state.mode === 'ready') {
      ready = true;
      window.clearTimeout(timeout);
      errorNotice.hidden = true;
      post('theme');
      const activeFrame = frame;
      requestAnimationFrame(() => { if (ready && frame === activeFrame) setFrameAvailable(true); });
    }
    mode = state.mode;
    updateStep(state);
    renderControls();
    if (pendingIntent) {
      const intent = pendingIntent;
      pendingIntent = null;
      request(intent);
    }
    pauseWhenUnseen();
  });

  function closeStepPanel(restoreFocus = false) {
    progress.open = false;
    if (showcase && stepPanel.matches(':popover-open')) stepPanel.hidePopover();
    if (restoreFocus) progressSummary.focus({ preventScroll: true });
  }
  function fitStepPanel() {
    if (!progress.open) {
      if (showcase && stepPanel.matches(':popover-open')) stepPanel.hidePopover();
      return;
    }
    const bounds = progressSummary.getBoundingClientRect();
    const below = window.innerHeight - bounds.bottom - 24;
    const above = bounds.top - parseFloat(getComputedStyle(toolbar).top) - 24;
    const upwards = below < 260 && above > below;
    progress.dataset.placement = upwards ? 'above' : 'below';
    stepPanel.style.maxHeight = `${Math.max(100, upwards ? above : below)}px`;
    if (showcase) {
      stepPanel.setAttribute('popover', 'manual');
      if (!stepPanel.matches(':popover-open')) stepPanel.showPopover();
      const width = stepPanel.getBoundingClientRect().width;
      stepPanel.style.left = `${Math.max(16, Math.min(bounds.right - width, window.innerWidth - width - 16))}px`;
      stepPanel.style.top = upwards ? 'auto' : `${bounds.bottom + 8}px`;
      stepPanel.style.bottom = upwards ? `${window.innerHeight - bounds.top + 8}px` : 'auto';
    }
  }
  progress.addEventListener('toggle', fitStepPanel);
  progress.addEventListener('focusout', event => {
    if (!(event.relatedTarget instanceof Node) || !progress.contains(event.relatedTarget)) closeStepPanel();
  });
  document.addEventListener('pointerdown', event => { if (!progress.contains(event.target as Node)) closeStepPanel(); });
  window.addEventListener('resize', fitStepPanel);
  window.addEventListener('scroll', fitStepPanel, { passive: true });
  root.addEventListener('scroll', fitStepPanel, { passive: true });
  function interact() {
    autoStarted = true;
    autoPaused = false;
    closeStepPanel();
    if (showcase && !expanded) root.dispatchEvent(new CustomEvent('tutorial:interact', { bubbles: true }));
  }
  playButton.addEventListener('click', () => {
    const intent = mode === 'playing' ? 'pause' : 'play';
    interact();
    explicitlyStarted = true;
    retainedByUser = intent === 'pause';
    syncResidency();
    request(intent);
  });
  manualButton.addEventListener('click', () => {
    interact();
    explicitlyStarted = retainedByUser = true;
    syncResidency();
    request('manual');
  });
  resetButton.addEventListener('click', () => {
    if (ready) {
      interact();
      retainedByUser = true;
      beginLoading(null);
      playButton.focus({ preventScroll: true });
    }
  });
  expandButton.addEventListener('click', () => { interact(); expanded ? closeExpanded() : openExpanded(); });

  function selectLesson(button: HTMLButtonElement) {
    if (button.getAttribute('aria-selected') === 'true') return;
    const tutorial = JSON.parse(button.dataset.fbdChoice!) as Tutorial & { posterSources: ThemeImageSources };
    const intent = mode === 'manual' || pendingIntent === 'manual' ? 'manual' : reducedMotion.matches ? null : 'play';
    interact();
    explicitlyStarted = true;
    retainedByUser = intent !== 'play';
    channel = `ncd-docs-demo:${tutorial.id}`;
    root.dataset.demoLesson = tutorial.id;
    const url = new URL(source, location.href);
    url.searchParams.set('lesson', tutorial.id);
    source = url.href;
    choices.forEach(choice => {
      choice.setAttribute('aria-selected', String(choice === button));
      choice.tabIndex = choice === button ? 0 : -1;
    });
    viewport.setAttribute('aria-labelledby', button.id);
    setText(root.querySelector<HTMLElement>('.fbd-heading')!, tutorial.title);
    setText(root.querySelector<HTMLElement>('.fbd-environment')!, tutorial.environment);
    frame.title = `NapCatQQ Desktop：${tutorial.summary}`;
    const poster = cover.querySelector<HTMLImageElement>('[data-theme-image]')!;
    setThemeImage(poster, tutorial.posterSources);
    poster.alt = `${tutorial.title}界面预览`;
    const list = root.querySelector<HTMLOListElement>('.fbd-steps')!;
    list.replaceChildren(...tutorial.steps.map((label, index) => {
      const item = document.createElement('li');
      item.dataset.fbdStep = String(index);
      const number = document.createElement('span');
      number.setAttribute('aria-hidden', 'true');
      number.textContent = String(index + 1).padStart(2, '0');
      const text = document.createElement('span');
      text.textContent = label;
      item.append(number, text);
      return item;
    }));
    steps = Array.from(list.children) as HTMLElement[];
    beginLoading(intent);
  }
  choices.forEach((button, index) => {
    button.addEventListener('click', () => selectLesson(button));
    button.addEventListener('keydown', event => {
      let next = index;
      if (event.key === 'ArrowRight') next = (index + 1) % choices.length;
      else if (event.key === 'ArrowLeft') next = (index - 1 + choices.length) % choices.length;
      else if (event.key === 'Home') next = 0;
      else if (event.key === 'End') next = choices.length - 1;
      else return;
      event.preventDefault();
      selectLesson(choices[next]);
      choices[next].focus({ preventScroll: true });
    });
  });

  function pauseWhenUnseen() {
    syncResidency();
    if (document.hidden || !isVisible()) {
      if (mode === 'playing') { autoPaused = showcase; request('pause'); }
      else if (pendingIntent === 'play') { autoPaused = showcase; pendingIntent = 'pause'; }
    } else if (showcase && !reducedMotion.matches && (!compactScreen.matches || explicitlyStarted)
      && !['pending', 'playing'].includes(document.documentElement.dataset.homeIntro ?? '')) {
      if (!autoStarted) { request('play'); autoStarted = mounted; }
      else if (autoPaused && mode === 'paused') { autoPaused = false; request('play'); }
    }
  }
  document.addEventListener('visibilitychange', pauseWhenUnseen);
  document.addEventListener('demo:expanded-change', pauseWhenUnseen);
  new IntersectionObserver((entries) => {
    const ratio = entries[0]?.intersectionRatio ?? 0;
    visibleRatio = ratio;
    // Separate enter/leave thresholds prevent tiny scroll changes from repeatedly pausing a preview.
    inViewport = showcase ? ratio >= (inViewport ? .15 : .35) : entries[0]?.isIntersecting ?? false;
    pauseWhenUnseen();
  }, { threshold: showcase ? [0, .15, .35] : 0 }).observe(root);
  if (showcase) {
    document.addEventListener('demo:budget-available', pauseWhenUnseen);
    document.addEventListener('home:intro-ready', pauseWhenUnseen);
    document.addEventListener('home:intro-start', () => {
      if (mode === 'playing') { autoPaused = true; request('pause'); }
      else if (pendingIntent === 'play') { autoPaused = true; pendingIntent = 'pause'; }
    });
    reducedMotion.addEventListener('change', () => {
      if (reducedMotion.matches) {
        autoPaused = false;
        if (mode === 'playing') request('pause');
        else if (pendingIntent === 'play') pendingIntent = 'pause';
      }
    });
    compactScreen.addEventListener('change', () => {
      if (compactScreen.matches && !explicitlyStarted) {
        if (mode === 'playing') { autoPaused = true; request('pause'); }
        else if (pendingIntent === 'play') { autoPaused = true; pendingIntent = 'pause'; }
      } else pauseWhenUnseen();
    });
  }

  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && progress.open) {
      event.preventDefault();
      closeStepPanel(true);
      return;
    }
    if (!expanded || event.defaultPrevented) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      closeExpanded();
      return;
    }
    if (event.key !== 'Tab') return;
    const elements = focusableElements();
    const first = elements[0], last = elements[elements.length - 1];
    if (!first || !last) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  });

  new ResizeObserver(resize).observe(viewport);
  new ResizeObserver(() => {
    root.style.setProperty('--fbd-toolbar-height', `${toolbar.offsetHeight}px`);
    resize();
    fitStepPanel();
  }).observe(toolbar);
  window.addEventListener('resize', resize);
  new MutationObserver(() => { if (ready) post('theme'); })
    .observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  setFrameAvailable(false);
  syncResidency();
  renderControls();
  resize();
}

document.querySelectorAll<HTMLElement>('[data-tutorial-demo]').forEach(setupDemo);

export {};
