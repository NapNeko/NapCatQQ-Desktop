type SearchEntry = { title: string; description: string; section: string; url: string; text: string };
const root = document.documentElement;
const themeButton = document.querySelector<HTMLButtonElement>('[data-theme-toggle]')!;
function syncTheme() {
  const dark = root.dataset.theme === 'dark';
  themeButton.setAttribute('aria-pressed', String(dark));
  themeButton.setAttribute('aria-label', dark ? '切换到浅色主题' : '切换到深色主题');
  themeButton.querySelector('[data-theme-icon]')!.textContent = dark ? '☀' : '☾';
  document.querySelectorAll<HTMLElement>('[data-shot]').forEach(shot => {
    const mode = shot.dataset.mode === 'auto' ? root.dataset.theme : shot.dataset.mode;
    shot.querySelectorAll<HTMLButtonElement>('[data-shot-theme]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.shotTheme === mode)));
  });
}
syncTheme();
themeButton.addEventListener('click', () => {
  root.dataset.theme = root.dataset.theme === 'dark' ? 'light' : 'dark';
  try { localStorage.setItem('ncd-docs-theme', root.dataset.theme); } catch { /* Reading remains available without storage. */ }
  syncTheme();
});
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', event => {
  let saved = false;
  try { saved = !!localStorage.getItem('ncd-docs-theme'); } catch { /* Follow system. */ }
  if (!saved) { root.dataset.theme = event.matches ? 'dark' : 'light'; syncTheme(); }
});

const sidebar = document.querySelector<HTMLElement>('#docs-sidebar')!;
const sidebarToggle = document.querySelector<HTMLButtonElement>('[data-sidebar-toggle]')!;
const backdrop = document.querySelector<HTMLButtonElement>('[data-sidebar-close]')!;
function setSidebar(open: boolean) {
  if (open) closeToc();
  sidebar.classList.toggle('is-open', open);
  sidebarToggle.setAttribute('aria-expanded', String(open));
  backdrop.hidden = !open;
}
sidebarToggle.addEventListener('click', () => setSidebar(sidebarToggle.getAttribute('aria-expanded') !== 'true'));
backdrop.addEventListener('click', () => setSidebar(false));
document.addEventListener('keydown', event => { if (event.key === 'Escape') setSidebar(false); });
const current = sidebar.querySelector<HTMLElement>('[aria-current=page]');
if (current && current.offsetTop > sidebar.clientHeight - 100) sidebar.scrollTop = current.offsetTop - sidebar.clientHeight / 2;

const toc = document.querySelector<HTMLDetailsElement>('[data-doc-toc]');
const tocSummary = toc?.querySelector<HTMLElement>('summary');
const tocCurrent = toc?.querySelector<HTMLElement>('[data-toc-current]');
const outlineLinks = [...document.querySelectorAll<HTMLAnchorElement>('[data-toc-link]')];
function anchorTarget(link: HTMLAnchorElement) {
  let id = link.hash.slice(1);
  try { id = decodeURIComponent(id); } catch { /* Keep literal IDs if decoding fails. */ }
  return document.getElementById(id);
}
const outlineTargets = outlineLinks.flatMap(link => {
  const target = anchorTarget(link);
  return target ? [{ link, target }] : [];
});
function closeToc(restoreFocus = false) {
  if (!toc?.open) return;
  toc.open = false;
  if (restoreFocus) tocSummary?.focus({ preventScroll: true });
}
function updateOutline() {
  if (!tocCurrent) return;
  const threshold = parseFloat(getComputedStyle(root).scrollPaddingTop) || 98;
  let active: typeof outlineTargets[number] | undefined;
  for (const item of outlineTargets) {
    if (item.target.getBoundingClientRect().top > threshold + 2) break;
    active = item;
  }
  if (window.scrollY > 0 && window.innerHeight + window.scrollY >= root.scrollHeight - 8) active = outlineTargets.at(-1);
  for (const item of outlineTargets) {
    if (item === active) item.link.setAttribute('aria-current', 'location');
    else item.link.removeAttribute('aria-current');
  }
  tocCurrent.textContent = active?.link.textContent || '概览';
  tocCurrent.title = tocCurrent.textContent;
}
let outlineFrame = 0;
function scheduleOutlineUpdate() {
  if (outlineFrame) return;
  outlineFrame = requestAnimationFrame(() => { outlineFrame = 0; updateOutline(); });
}
if (toc) {
  toc.addEventListener('click', event => {
    const link = (event.target as Element).closest<HTMLAnchorElement>('a');
    if (!link) return;
    const target = anchorTarget(link);
    closeToc();
    if (target) {
      const addedTabIndex = !target.hasAttribute('tabindex');
      if (addedTabIndex) target.tabIndex = -1;
      target.focus({ preventScroll: true });
      if (addedTabIndex) target.addEventListener('blur', () => target.removeAttribute('tabindex'), { once: true });
    }
  });
  toc.addEventListener('focusout', event => {
    if (!(event.relatedTarget instanceof Node) || !toc.contains(event.relatedTarget)) closeToc();
  });
  document.addEventListener('pointerdown', event => { if (!toc.contains(event.target as Node)) closeToc(); });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape' && toc.open) { event.preventDefault(); closeToc(true); }
  });
  window.addEventListener('scroll', scheduleOutlineUpdate, { passive: true });
  window.addEventListener('resize', scheduleOutlineUpdate);
  window.addEventListener('hashchange', scheduleOutlineUpdate);
  window.addEventListener('load', scheduleOutlineUpdate);
  const prose = document.querySelector('.doc-prose');
  if (prose) new ResizeObserver(scheduleOutlineUpdate).observe(prose);
  updateOutline();
}

const lightbox = document.querySelector<HTMLDialogElement>('[data-lightbox]')!;
document.querySelectorAll<HTMLElement>('[data-shot]').forEach(shot => {
  shot.querySelectorAll<HTMLButtonElement>('[data-shot-theme]').forEach(button => button.addEventListener('click', () => { shot.dataset.mode = button.dataset.shotTheme; syncTheme(); }));
  shot.querySelector('[data-shot-open]')!.addEventListener('click', () => {
    const visible = [...shot.querySelectorAll<HTMLImageElement>('img')].find(img => getComputedStyle(img).display !== 'none')!;
    const image = lightbox.querySelector('img')!;
    image.src = visible.src;
    image.alt = visible.alt;
    lightbox.querySelector('p')!.textContent = `${visible.alt} · 演示数据`;
    lightbox.showModal();
  });
});
lightbox.querySelector('[data-lightbox-close]')!.addEventListener('click', () => lightbox.close());
lightbox.querySelector('img')!.addEventListener('click', () => lightbox.close());
for (const dialog of document.querySelectorAll<HTMLDialogElement>('dialog')) {
  dialog.addEventListener('click', event => {
    if (event.target !== dialog) return;
    const bounds = dialog.getBoundingClientRect();
    if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) dialog.close();
  });
}

document.querySelectorAll<HTMLPreElement>('.doc-prose pre').forEach(pre => {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'copy-code';
  button.textContent = '复制';
  button.setAttribute('aria-label', '复制代码');
  button.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(pre.querySelector('code')?.textContent || ''); button.textContent = '已复制'; }
    catch { button.textContent = '请手动选择复制'; }
    setTimeout(() => { button.textContent = '复制'; }, 2000);
  });
  pre.prepend(button);
});

const searchDialog = document.querySelector<HTMLDialogElement>('[data-search-dialog]')!;
const searchInput = document.querySelector<HTMLInputElement>('#docs-search-input')!;
const results = document.querySelector<HTMLElement>('.search-results')!;
const status = document.querySelector<HTMLElement>('.search-status')!;
let searchData: SearchEntry[] | undefined;
let selected = -1;
let dataPromise: Promise<SearchEntry[]> | undefined;
async function openSearch() {
  setSidebar(false);
  closeToc();
  if (!searchDialog.open) searchDialog.showModal();
  searchInput.focus();
  if (searchData) return;
  status.textContent = '正在加载搜索索引…';
  try {
    dataPromise ||= fetch(`${import.meta.env.BASE_URL}docs/search.json`).then(response => {
      if (!response.ok) throw new Error('Search index unavailable');
      return response.json();
    });
    searchData = await dataPromise;
    showResults();
  } catch {
    dataPromise = undefined;
    status.textContent = '搜索暂时无法加载，请稍后重试。也可以通过章节目录查找。';
  }
}
document.querySelector('[data-search-open]')!.addEventListener('click', openSearch);
document.querySelector('[data-search-close]')!.addEventListener('click', () => searchDialog.close());
document.addEventListener('keydown', event => {
  const typing = (event.target as HTMLElement)?.closest('input, textarea, [contenteditable=true]');
  if (((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') || (event.key === '/' && !typing && !lightbox.open)) { event.preventDefault(); openSearch(); }
});
function selectResult(index: number) {
  const links = [...results.querySelectorAll<HTMLAnchorElement>('a')];
  selected = links.length ? (index + links.length) % links.length : -1;
  links.forEach((link, i) => link.classList.toggle('is-selected', i === selected));
  links[selected]?.scrollIntoView({ block: 'nearest' });
}
function showResults() {
  const query = searchInput.value.trim().toLowerCase();
  results.replaceChildren();
  selected = -1;
  if (!searchData) return;
  if (!query) { status.textContent = '输入关键词，搜索全部教程与功能说明。试试「创建 Bot」「反向 WebSocket」或「备份」。'; return; }
  const terms = query.split(/\s+/).filter(Boolean);
  const matches = searchData.map(item => {
    const title = item.title.toLowerCase(), description = item.description.toLowerCase(), text = item.text.toLowerCase();
    if (!terms.every(term => `${title} ${description} ${text}`.includes(term))) return { item, score: 0 };
    const score = terms.reduce((sum, term) => sum + (title.includes(term) ? 20 : 0) + (description.includes(term) ? 8 : 0) + (text.includes(term) ? 1 : 0), 0);
    return { item, score };
  }).filter(hit => hit.score > 0).sort((a, b) => b.score - a.score);
  const hits = matches.slice(0, 12);
  status.textContent = matches.length ? `找到 ${matches.length} 篇相关文档${matches.length > 12 ? '，显示前 12 篇' : ''}` : '没有找到相关文档。可以换用按钮名称或更短的关键词。';
  for (const { item } of hits) {
    const link = document.createElement('a');
    link.href = item.url;
    link.className = 'search-result';
    const section = document.createElement('small'); section.textContent = item.section;
    const title = document.createElement('strong'); title.textContent = item.title;
    const snippet = document.createElement('p');
    const start = Math.max(0, item.text.toLowerCase().indexOf(terms[0]) - 38);
    snippet.textContent = `${start ? '…' : ''}${item.text.slice(start, start + 135)}${start + 135 < item.text.length ? '…' : ''}`;
    link.append(section, title, snippet);
    const itemWrapper = document.createElement('div');
    itemWrapper.setAttribute('role', 'listitem');
    itemWrapper.append(link);
    results.append(itemWrapper);
  }
  selectResult(0);
}
searchInput.addEventListener('input', showResults);
searchInput.addEventListener('keydown', event => {
  if (event.key === 'Escape') { event.preventDefault(); searchDialog.close(); return; }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); selectResult(selected + (event.key === 'ArrowDown' ? 1 : -1)); }
  if (event.key === 'Enter') { const link = results.querySelector<HTMLAnchorElement>('.is-selected'); if (link) { event.preventDefault(); location.href = link.href; } }
});
