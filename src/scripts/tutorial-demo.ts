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
  const channel = `ncd-docs-demo:${root.dataset.demoLesson}`;
  const frame = root.querySelector<HTMLIFrameElement>('[data-fbd-frame]')!;
  const viewport = root.querySelector<HTMLElement>('[data-fbd-viewport]')!;
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
  const closeButton = root.querySelector<HTMLButtonElement>('[data-fbd-close]')!;
  const modeLabel = root.querySelector<HTMLElement>('[data-fbd-mode]')!;
  const position = root.querySelector<HTMLElement>('[data-fbd-position]')!;
  const title = root.querySelector<HTMLElement>('[data-fbd-title]')!;
  const detail = root.querySelector<HTMLElement>('[data-fbd-detail]')!;
  const errorNotice = root.querySelector<HTMLElement>('[data-fbd-error]')!;
  const panHint = root.querySelector<HTMLElement>('[data-fbd-pan]')!;
  const steps = Array.from(root.querySelectorAll<HTMLElement>('[data-fbd-step]'));
  const anchor = root.closest<HTMLElement>('[data-tutorial-anchor]')!;
  const source = root.dataset.demoSrc!;
  const demoWidth = Number(frame.width);
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
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
  let inViewport = true;

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
  }

  function updateStep(state: DemoState) {
    setText(position, `第 ${state.step + 1} 步 / ${steps.length}`);
    setText(title, state.title);
    setText(detail, state.detail);
    steps.forEach((step, index) => {
      step.classList.toggle('is-done', index < state.step || state.mode === 'complete');
      if (index === state.step) step.setAttribute('aria-current', 'step');
      else step.removeAttribute('aria-current');
    });
  }

  function setFrameAvailable(available: boolean) {
    cover.hidden = available;
    if (available) frame.removeAttribute('aria-hidden');
    else frame.setAttribute('aria-hidden', 'true');
    frame.tabIndex = available ? 0 : -1;
    frame.inert = !available;
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
    ready = false;
    pendingIntent = intent;
    mode = 'loading';
    setText(errorNotice, '');
    errorNotice.hidden = true;
    setFrameAvailable(false);
    setText(position, '准备演示');
    setText(title, '正在加载软件界面');
    setText(detail, '演示中的操作只在演示环境内生效。');
    steps.forEach(step => { step.removeAttribute('aria-current'); step.classList.remove('is-done'); });
    renderControls();
    armTimeout();
    // A fresh document also clears private UI caches and timers in the reused frontend.
    mounted = true;
    frame.src = source;
  }

  function request(intent: Intent) {
    if (!mounted || mode === 'error' || (intent === 'play' && (mode === 'manual' || mode === 'complete'))) {
      beginLoading(intent);
      return;
    }
    pendingIntent = intent;
    if (!ready) { renderControls(); return; }
    pendingIntent = null;
    if (intent !== 'pause') {
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
    const scale = Math.min(1, expanded ? Math.max(.85, width / demoWidth) : width / demoWidth);
    root.style.setProperty('--fbd-scale', String(scale));
    const overflow = demoWidth * scale > width + 1;
    panHint.hidden = !overflow && scale >= .6;
    setText(panHint, overflow ? '左右滑动查看窗口，或收起回到完整预览。' : '界面已缩小，点「放大」操作。');
    viewport.setAttribute('aria-label', overflow ? '软件演示窗口，可左右滚动' : '软件演示窗口');
    viewport.tabIndex = overflow ? 0 : -1;
  }

  function focusableElements() {
    return Array.from(root.querySelectorAll<HTMLElement>('button:not([disabled]), a[href], input:not([disabled]), iframe, [tabindex]'))
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
    root.dataset.expanded = 'false';
    root.setAttribute('role', 'region');
    root.removeAttribute('aria-modal');
    closeButton.hidden = true;
    expandButton.setAttribute('aria-expanded', 'false');
    setText(expandLabel, '放大');
    anchor.style.removeProperty('height');
    document.documentElement.style.overflow = previousOverflow;
    inertElements.forEach(({ element, wasInert }) => { element.inert = wasInert; });
    inertElements = [];
    resize();
    if (previousFocus?.isConnected) previousFocus.focus({ preventScroll: true });
    else expandButton.focus({ preventScroll: true });
  }

  function openExpanded() {
    if (expanded) return;
    previousFocus = expandButton;
    anchor.style.height = `${anchor.getBoundingClientRect().height}px`;
    previousOverflow = document.documentElement.style.overflow;
    expanded = true;
    root.dataset.expanded = 'true';
    root.setAttribute('role', 'dialog');
    root.setAttribute('aria-modal', 'true');
    closeButton.hidden = false;
    expandButton.setAttribute('aria-expanded', 'true');
    setText(expandLabel, '收起');
    document.documentElement.style.overflow = 'hidden';
    isolateExpandedView();
    resize();
    closeButton.focus({ preventScroll: true });
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
      requestAnimationFrame(() => { if (ready) setFrameAvailable(true); });
    }
    mode = state.mode;
    updateStep(state);
    renderControls();
    if (pendingIntent) {
      const intent = pendingIntent;
      pendingIntent = null;
      request(intent);
    }
  });

  playButton.addEventListener('click', () => request(mode === 'playing' ? 'pause' : 'play'));
  manualButton.addEventListener('click', () => request('manual'));
  resetButton.addEventListener('click', () => { if (ready) beginLoading(null); });
  expandButton.addEventListener('click', () => expanded ? closeExpanded() : openExpanded());
  closeButton.addEventListener('click', closeExpanded);
  frame.addEventListener('error', () => showError('演示加载失败。请重试，或继续阅读图文步骤。'));

  function pauseWhenUnseen() {
    if (document.hidden || !inViewport) {
      if (mode === 'playing') request('pause');
      else if (pendingIntent === 'play') pendingIntent = 'pause';
    }
  }
  document.addEventListener('visibilitychange', pauseWhenUnseen);
  new IntersectionObserver((entries) => {
    inViewport = entries[0]?.isIntersecting ?? false;
    pauseWhenUnseen();
  }).observe(root);

  document.addEventListener('keydown', (event) => {
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
  new MutationObserver(() => { if (ready) post('theme'); })
    .observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  setFrameAvailable(false);
  renderControls();
  resize();
}

document.querySelectorAll<HTMLElement>('[data-tutorial-demo]').forEach(setupDemo);

export {};
