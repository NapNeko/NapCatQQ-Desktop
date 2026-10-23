import { subscribeScenario } from './bot.mock';
import type { DemoDriver, DemoLesson, DemoTarget } from './lesson-types';

type Mode = 'ready' | 'playing' | 'paused' | 'manual' | 'complete' | 'error';
type Adapter = { reset(): void; theme(value: 'light' | 'dark'): void };

export function installPlayer(lesson: DemoLesson, adapter: Adapter) {
  const channel = `ncd-docs-demo:${lesson.id}`;
  let mode: Mode = 'ready';
  let step = 0;
  let generation = 0;
  let nextAction = 0;
  let highlighted: HTMLElement | null = null;
  let disposed = false;
  let observerFrame = 0;
  const cursor = () => document.querySelector<HTMLElement>('.demo-cursor');
  const delay = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms));
  const visible = (element: Element) => element.isConnected && element.getClientRects().length > 0;
  const enabled = (element: HTMLElement | null): element is HTMLElement => !!element && visible(element)
    && !element.matches(':disabled, [aria-disabled="true"]');

  function report(error?: string) {
    if (disposed) return;
    const [title, detail] = lesson.steps[step];
    window.parent.postMessage({ channel, type: 'state', mode, step, title, detail, ...(error ? { error } : {}) }, location.origin);
  }

  function clearHighlight() {
    highlighted?.removeAttribute('data-demo-target');
    highlighted = null;
  }

  function setMode(value: Mode) {
    mode = value;
    if (cursor()) cursor()!.hidden = mode !== 'playing';
    if (mode !== 'playing' && mode !== 'paused') clearHighlight();
    report();
  }

  async function gate(token: number) {
    while (!disposed && token === generation && (mode === 'paused' || (mode === 'playing' && document.hidden))) await delay(60);
    if (disposed || token !== generation || mode !== 'playing') throw new DOMException('演示已接管', 'AbortError');
  }

  function driver(token: number): DemoDriver {
    async function wait(ms: number) {
      for (let remaining = ms; remaining > 0; remaining -= 60) {
        await gate(token);
        await delay(Math.min(60, remaining));
      }
      await gate(token);
    }

    async function until(check: () => boolean, message = '页面没有进入预期步骤，请重置后再试。') {
      for (let i = 0; i < 240; i++) {
        await gate(token);
        if (check()) return;
        await wait(60);
      }
      throw new Error(message);
    }

    async function find(get: DemoTarget, requireEnabled = false) {
      let target: HTMLElement | null = null;
      await until(() => {
        target = get();
        return !!target && visible(target) && (!requireEnabled || enabled(target));
      });
      return target!;
    }

    function pointAt(element: HTMLElement) {
      // Scroll the software's own panel without moving its embedding lesson.
      for (let parent = element.parentElement; parent; parent = parent?.parentElement ?? null) {
        if (!/(auto|scroll)/.test(getComputedStyle(parent).overflowY) || parent.scrollHeight <= parent.clientHeight) continue;
        const bounds = parent.getBoundingClientRect();
        const target = element.getBoundingClientRect();
        if (target.top < bounds.top + 10) parent.scrollTop += target.top - bounds.top - 10;
        else if (target.bottom > bounds.bottom - 10) parent.scrollTop += target.bottom - bounds.bottom + 10;
        break;
      }
      clearHighlight();
      highlighted = element;
      element.setAttribute('data-demo-target', '');
      const rect = element.getBoundingClientRect();
      const x = Math.max(10, Math.min(innerWidth - 60, rect.left + Math.min(rect.width * .65, 200)));
      const y = Math.max(10, Math.min(innerHeight - 65, rect.top + rect.height * .55));
      if (cursor()) cursor()!.style.transform = `translate(${x}px, ${y}px)`;
    }

    async function point(get: DemoTarget) {
      pointAt(await find(get));
      await wait(650);
    }

    function press(element: HTMLElement) {
      cursor()?.classList.add('is-clicking');
      const rect = element.getBoundingClientRect();
      const pointer = { bubbles: true, cancelable: true, button: 0, pointerId: 1, pointerType: 'mouse',
        clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2 };
      // Radix ignores a menu release until the pointer has moved away from its trigger.
      element.dispatchEvent(new PointerEvent('pointermove', { ...pointer, buttons: 0 }));
      element.dispatchEvent(new PointerEvent('pointerdown', { ...pointer, buttons: 1 }));
      element.dispatchEvent(new PointerEvent('pointerup', { ...pointer, buttons: 0 }));
      element.click();
    }

    async function settleClick() {
      try { await wait(350); }
      finally { cursor()?.classList.remove('is-clicking'); }
    }

    async function click(get: DemoTarget) {
      await point(get);
      await find(get, true);
      const element = get();
      if (!enabled(element)) return click(get);
      if (element !== highlighted) pointAt(element);
      press(element);
      await settleClick();
    }

    async function select(trigger: DemoTarget, option: DemoTarget, selected: () => boolean) {
      await point(trigger);
      for (let attempt = 0; attempt < 6; attempt++) {
        if (selected()) return;
        await find(trigger, true);
        const control = trigger();
        if (!enabled(control)) continue;
        if (control.getAttribute('aria-expanded') !== 'true') {
          press(control);
          await settleClick();
        }
        await until(() => trigger()?.getAttribute('aria-expanded') !== 'true' || enabled(option()), '下拉选项尚未打开');
        let item = option();
        if (trigger()?.getAttribute('aria-expanded') !== 'true' || !enabled(item)) continue;
        pointAt(item);
        await wait(650);
        // Pausing can blur the iframe and dismiss Radix's portal; reopen instead of clicking a detached item.
        item = option();
        if (trigger()?.getAttribute('aria-expanded') !== 'true' || !enabled(item)) continue;
        if (item !== highlighted) pointAt(item);
        press(item);
        await settleClick();
        await until(() => trigger()?.getAttribute('aria-expanded') === 'false', '下拉选择尚未完成');
        // Blur also closes Radix menus; only the selected value proves the click succeeded.
        if (selected()) return;
      }
      throw new Error('未能完成下拉选择，请重新演示或切换为自己操作。');
    }

    async function type(get: DemoTarget, value: string) {
      await point(get);
      let element: HTMLInputElement | HTMLTextAreaElement | null = null;
      for (let length = value.length ? 1 : 0; length <= value.length; length++) {
        await gate(token);
        await find(get, true);
        const current = get();
        if (!enabled(current)) { length -= 1; continue; }
        if (!(current instanceof HTMLInputElement || current instanceof HTMLTextAreaElement)) {
          throw new Error('这个控件不能填写文本，请重置演示。');
        }
        if (element !== current) { element = current; element.focus({ preventScroll: true }); }
        const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
        const setter = Object.getOwnPropertyDescriptor(prototype, 'value')!.set!;
        setter.call(element, value.slice(0, length));
        element.dispatchEvent(new Event('input', { bubbles: true }));
        await wait(65);
      }
      element?.blur();
      await wait(300);
    }

    return {
      by: selector => () => document.querySelector<HTMLElement>(selector),
      button: text => () => [...document.querySelectorAll<HTMLButtonElement>('button')]
        .find(element => visible(element) && element.textContent?.trim() === text) ?? null,
      input: text => () => {
        const normal = (value: string) => value.replace(/\*/g, '').trim();
        const label = [...document.querySelectorAll('label')].find(element => normal(element.textContent ?? '') === normal(text));
        const labelled = label?.htmlFor ? document.getElementById(label.htmlFor) : label?.querySelector<HTMLElement>('input,textarea');
        return labelled ?? [...document.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('input,textarea')]
          .find(element => normal(element.getAttribute('aria-label') ?? '') === normal(text)) ?? null;
      },
      text: (text, selector = 'button,[role="tab"],label,h1,h2,h3,span,p') => () => [...document.querySelectorAll<HTMLElement>(selector)]
        .find(element => visible(element) && element.textContent?.trim().includes(text)) ?? null,
      click, select, type, point, find, wait, until,
    };
  }

  async function play() {
    if (mode === 'playing') return;
    if (mode === 'paused') { setMode('playing'); return; }
    const token = ++generation;
    if (mode === 'manual' || mode === 'complete' || mode === 'error') {
      nextAction = 0; step = 0;
      setMode('playing');
      adapter.reset();
    }
    setMode('playing');
    const d = driver(token);
    try {
      await d.wait(200);
      while (nextAction < lesson.actions.length) {
        step = nextAction;
        report();
        await lesson.actions[nextAction](d);
        await gate(token);
        nextAction += 1;
      }
      clearHighlight();
      setMode('complete');
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') return;
      setMode('error');
      report(error instanceof Error ? error.message : String(error));
    }
  }

  function observeProgress() {
    if (mode !== 'manual' || !lesson.observe) return;
    const next = Math.max(0, Math.min(lesson.steps.length - 1, lesson.observe()));
    if (step !== next) { step = next; report(); }
  }

  function takeOver() {
    generation += 1;
    setMode('manual');
    observeProgress();
  }

  function message(event: MessageEvent) {
    if (event.source !== window.parent || event.origin !== location.origin || event.data?.channel !== channel || event.data.type !== 'command') return;
    const data = event.data;
    if (data.command === 'theme' && (data.theme === 'light' || data.theme === 'dark')) adapter.theme(data.theme);
    else if (data.command === 'play') void play();
    else if (data.command === 'pause' && mode === 'playing') setMode('paused');
    else if (data.command === 'manual') takeOver();
    else if (data.command === 'reset') {
      generation += 1; nextAction = 0; step = 0;
      mode = 'ready';
      clearHighlight();
      adapter.reset();
      step = 0;
      setMode('ready');
    }
  }

  function interaction(event: Event) {
    if (event.isTrusted && (mode === 'playing' || mode === 'paused' || mode === 'ready' || mode === 'complete')) takeOver();
    const hasOpenPopup = [...document.querySelectorAll('[role="dialog"], [role="menu"], [role="listbox"]')].some(visible);
    if (event instanceof KeyboardEvent && event.key === 'Escape' && !hasOpenPopup) {
      window.parent.postMessage({ channel, type: 'escape' }, location.origin);
    }
  }

  const observer = new MutationObserver(() => {
    if (observerFrame || mode !== 'manual') return;
    observerFrame = requestAnimationFrame(() => { observerFrame = 0; observeProgress(); });
  });
  observer.observe(document.getElementById('root')!, { subtree: true, childList: true, attributes: true, attributeFilter: ['aria-checked', 'aria-selected', 'data-state'] });
  const unsubscribe = subscribeScenario(observeProgress);
  window.addEventListener('message', message);
  document.addEventListener('pointerdown', interaction, true);
  document.addEventListener('keydown', interaction, true);
  document.addEventListener('input', observeProgress);
  report();
  return () => {
    disposed = true; generation += 1;
    observer.disconnect(); unsubscribe();
    cancelAnimationFrame(observerFrame);
    window.removeEventListener('message', message);
    document.removeEventListener('pointerdown', interaction, true);
    document.removeEventListener('keydown', interaction, true);
    document.removeEventListener('input', observeProgress);
    clearHighlight();
  };
}
