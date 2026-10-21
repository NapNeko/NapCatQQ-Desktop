import { readingProgress } from '../lib/reading-outline';

const root = document.documentElement;
const navigation = document.querySelector<HTMLDetailsElement>('[data-reading-nav]');
const summary = navigation?.querySelector<HTMLElement>('summary');
const current = navigation?.querySelector<HTMLElement>('[data-reading-current]');
const position = navigation?.querySelector<HTMLElement>('[data-reading-position]');
const percent = navigation?.querySelector<HTMLElement>('[data-reading-percent]');
const prose = document.querySelector<HTMLElement>('.doc-prose');

function anchorTarget(link: HTMLAnchorElement) {
  let id = link.hash.slice(1);
  try { id = decodeURIComponent(id); } catch { /* Keep literal IDs if decoding fails. */ }
  return document.getElementById(id);
}
const targets = [...document.querySelectorAll<HTMLAnchorElement>('[data-reading-link]')].flatMap(link => {
  const target = anchorTarget(link);
  return target ? [{ link, target }] : [];
});

export function closeReadingNav(restoreFocus = false) {
  if (!navigation?.open) return;
  navigation.open = false;
  if (restoreFocus) summary?.focus({ preventScroll: true });
}

function updateReadingPosition() {
  if (!navigation || !current || !position || !prose) return;
  const threshold = parseFloat(getComputedStyle(root).scrollPaddingTop) || 96;
  const menuHeight = navigation.open && !matchMedia('(min-width: 1440px)').matches
    ? navigation.querySelector<HTMLElement>('.reading-links')?.getBoundingClientRect().height || 0 : 0;
  let active: typeof targets[number] | undefined;
  for (const item of targets) {
    if (item.target.getBoundingClientRect().top - menuHeight > threshold + 2) break;
    active = item;
  }
  if (scrollY > 0 && innerHeight + scrollY >= root.scrollHeight - 8) active = targets.at(-1);
  for (const item of targets) {
    if (item === active) item.link.setAttribute('aria-current', 'location');
    else item.link.removeAttribute('aria-current');
    item.link.toggleAttribute('data-current-section', item.target.id === active?.link.dataset.section);
  }
  current.textContent = active?.link.dataset.headingTitle || '开始阅读';
  current.title = current.textContent;
  position.textContent = active?.link.dataset.position || '阅读位置';

  // An expanded in-flow menu must not count as additional article content.
  const bounds = prose.getBoundingClientRect();
  const progress = readingProgress(scrollY, bounds.top + scrollY - menuHeight, bounds.bottom + scrollY - menuHeight, innerHeight, threshold);
  navigation.style.setProperty('--reading-progress', String(progress / 100));
  if (percent) percent.textContent = `${progress}%`;
  summary?.setAttribute('aria-label', `${position.textContent}，${current.textContent}，阅读进度 ${progress}%，${navigation.open ? '收起' : '展开'}章节导航`);
}
let frame = 0;
function scheduleUpdate() {
  if (frame) return;
  frame = requestAnimationFrame(() => { frame = 0; updateReadingPosition(); });
}
if (navigation) {
  navigation.addEventListener('toggle', () => {
    scheduleUpdate();
    if (!navigation.open) return;
    const links = navigation.querySelector<HTMLElement>('.reading-links');
    const active = navigation.querySelector<HTMLElement>('[aria-current=location]');
    if (links && active) links.scrollTop += active.getBoundingClientRect().top - links.getBoundingClientRect().top - links.clientHeight / 3;
  });
  navigation.addEventListener('click', event => {
    const link = (event.target as Element).closest<HTMLAnchorElement>('a');
    if (!link || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    const target = anchorTarget(link);
    closeReadingNav();
    if (target) {
      const temporaryFocus = !target.hasAttribute('tabindex');
      if (temporaryFocus) target.tabIndex = -1;
      target.focus({ preventScroll: true });
      if (temporaryFocus) target.addEventListener('blur', () => target.removeAttribute('tabindex'), { once: true });
    }
  });
  navigation.addEventListener('focusout', event => {
    if (!(event.relatedTarget instanceof Node) || !navigation.contains(event.relatedTarget)) closeReadingNav();
  });
  document.addEventListener('pointerdown', event => { if (!navigation.contains(event.target as Node)) closeReadingNav(); });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape' && navigation.open) { event.preventDefault(); closeReadingNav(true); }
  });
  for (const name of ['scroll', 'resize', 'hashchange', 'load']) window.addEventListener(name, scheduleUpdate, { passive: true });
  if (prose) new ResizeObserver(scheduleUpdate).observe(prose);
  updateReadingPosition();
}
