const root = document.querySelector<HTMLElement>('[data-changelog]');

if (root) {
  const releases = Array.from(root.querySelectorAll<HTMLElement>('[data-release]'));
  const products = Array.from(root.querySelectorAll<HTMLButtonElement>('button[data-product]'));
  const preview = root.querySelector<HTMLInputElement>('[data-preview-toggle]');
  const links = Array.from(root.querySelectorAll<HTMLAnchorElement>('a[data-release-link]'));
  const picker = root.querySelector<HTMLSelectElement>('[data-release-select]');
  const options = picker ? Array.from(picker.options) : [];
  const statuses = root.querySelectorAll<HTMLElement>('[data-changelog-status]');
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  let product = 'desktop';
  let includePreview = false;
  let frame = 0;
  let activeTag = '';
  let animateChange: (() => void) | undefined;

  const tagOf = (release: HTMLElement) => release.dataset.releaseTag || release.id;
  function fromHash(hash: string) {
    try {
      const tag = decodeURIComponent(hash.replace(/^#/, ''));
      return releases.find(release => tagOf(release) === tag);
    } catch { return undefined; }
  }
  const visible = () => releases.filter(release => !release.hidden);
  const report = (message: string) => statuses.forEach(status => { status.textContent = message; });
  const detailsFor = (release: HTMLElement) => release.matches('details')
    ? release as HTMLDetailsElement
    : release.querySelector<HTMLDetailsElement>('[data-release-details]')
      ?? release.closest<HTMLDetailsElement>('[data-release-details]');

  function activate(release: HTMLElement | undefined) {
    if (!release) return;
    const tag = tagOf(release);
    if (tag === activeTag) return;
    activeTag = tag;
    root!.dataset.currentRelease = tag;
    links.forEach(link => {
      if (fromHash(link.hash) === release) link.setAttribute('aria-current', 'location');
      else link.removeAttribute('aria-current');
    });
    if (picker) picker.value = options.find(option => option.value.replace(/^#/, '') === tag)?.value ?? '';
    const current = links.find(link => link.getAttribute('aria-current') === 'location');
    const index = current?.closest<HTMLElement>('[data-changelog-index]');
    if (current && index) {
      const offset = current.getBoundingClientRect().top - index.getBoundingClientRect().top + index.scrollTop;
      index.style.setProperty('--cl-index-position', `${offset}px`);
    }
  }

  function filter(animate = false) {
    root!.dataset.activeProduct = product;
    root!.dataset.includePreview = String(includePreview);
    products.forEach(button => {
      const selected = button.dataset.product === product;
      button.setAttribute('aria-selected', String(selected));
      button.tabIndex = selected ? 0 : -1;
    });
    const selectedTab = products.find(button => button.dataset.product === product);
    if (selectedTab?.id) root!.querySelector('#release-feed')?.setAttribute('aria-labelledby', selectedTab.id);
    if (preview) preview.checked = includePreview;
    releases.forEach(release => {
      release.hidden = release.dataset.product !== product
        || (!includePreview && release.dataset.prerelease === 'true');
    });
    links.forEach(link => {
      const release = fromHash(link.hash);
      const item = link.closest<HTMLElement>('[data-release-index]') ?? link;
      item.hidden = !release || release.hidden;
    });
    options.forEach(option => {
      const release = fromHash(`#${option.value.replace(/^#/, '')}`);
      option.hidden = !release || release.hidden;
      option.disabled = option.hidden;
    });
    root!.querySelectorAll<HTMLElement>('[data-product-panel]').forEach(panel => {
      panel.hidden = panel.dataset.productPanel !== product;
    });
    const list = visible();
    const empty = root!.querySelector<HTMLElement>('[data-changelog-empty]');
    if (empty) empty.hidden = list.length !== 0;
    const current = fromHash(location.hash);
    activeTag = '';
    activate(current && !current.hidden ? current : list[0]);
    report(`${product === 'watch' ? 'ncd-watch' : 'Desktop'}，共 ${list.length} 个版本`);
    if (animate) animateChange?.();
    queueReadPosition();
  }

  function jump(release: HTMLElement, behavior: ScrollBehavior = 'smooth') {
    product = release.dataset.product || 'desktop';
    if (release.dataset.prerelease === 'true') includePreview = true;
    filter();
    const details = detailsFor(release);
    if (details) details.open = true;
    activate(release);
    requestAnimationFrame(() => release.scrollIntoView({ block: 'start', behavior: reducedMotion.matches ? 'instant' : behavior }));
  }

  function setHash(release: HTMLElement, replace = false) {
    const url = new URL(location.href);
    url.hash = tagOf(release);
    if (replace) history.replaceState(null, '', url);
    else if (location.hash !== url.hash) history.pushState(null, '', url);
  }

  function showFilterStart() {
    const first = visible()[0];
    if (!first) return;
    setHash(first, true);
    const controls = root!.querySelector<HTMLElement>('[data-changelog-controls]');
    if (first.getBoundingClientRect().top < (controls?.getBoundingClientRect().bottom ?? 0)) {
      jump(first, 'instant');
    }
  }

  products.forEach((button, index) => {
    button.addEventListener('click', () => {
      if (product === button.dataset.product) return;
      product = button.dataset.product!;
      filter(true);
      showFilterStart();
    });
    button.addEventListener('keydown', event => {
      let next = index;
      if (event.key === 'ArrowRight') next = (index + 1) % products.length;
      else if (event.key === 'ArrowLeft') next = (index - 1 + products.length) % products.length;
      else if (event.key === 'Home') next = 0;
      else if (event.key === 'End') next = products.length - 1;
      else return;
      event.preventDefault();
      products[next].click();
      products[next].focus({ preventScroll: true });
    });
  });
  preview?.addEventListener('change', () => {
    includePreview = preview.checked;
    filter(true);
    if (fromHash(location.hash)?.hidden) showFilterStart();
  });
  links.forEach(link => link.addEventListener('click', event => {
    const release = fromHash(link.hash);
    if (!release || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    setHash(release);
    jump(release);
  }));
  picker?.addEventListener('change', () => {
    const release = fromHash(`#${picker.value.replace(/^#/, '')}`);
    if (!release) return;
    setHash(release);
    jump(release);
  });
  const followHash = () => {
    const release = fromHash(location.hash);
    if (release) jump(release, 'instant');
  };
  window.addEventListener('hashchange', followHash);
  window.addEventListener('popstate', followHash);

  function readPosition() {
    frame = 0;
    const list = visible();
    const controls = root!.querySelector<HTMLElement>('[data-changelog-controls]');
    const threshold = Math.max(150, (controls?.getBoundingClientRect().bottom ?? 110) + 36);
    let current = list[0];
    for (const release of list) {
      if (release.getBoundingClientRect().top <= threshold) current = release;
      else break;
    }
    activate(current);
    const timeline = root!.querySelector<HTMLElement>('[data-release-timeline]');
    if (timeline) {
      const bounds = timeline.getBoundingClientRect();
      const progress = Math.min(1, Math.max(0, (threshold - bounds.top) / Math.max(1, bounds.height)));
      timeline.style.setProperty('--cl-read-progress', String(progress));
      const bar = document.querySelector<HTMLElement>('.cl-page-progress > span');
      if (bar) bar.style.transform = `scaleX(${progress})`;
    }
  }
  function queueReadPosition() {
    if (!frame) frame = requestAnimationFrame(readPosition);
  }
  window.addEventListener('scroll', queueReadPosition, { passive: true });
  window.addEventListener('resize', queueReadPosition);
  root.querySelectorAll<HTMLDetailsElement>('details').forEach(details => details.addEventListener('toggle', queueReadPosition));

  root.querySelectorAll<HTMLButtonElement>('[data-copy-release]').forEach(button => {
    const label = button.querySelector<HTMLElement>('[data-copy-label]');
    const original = label?.textContent ?? '';
    let timeout: number | undefined;
    button.addEventListener('click', async () => {
      const release = releases.find(item => tagOf(item) === button.dataset.copyRelease);
      if (!release) return;
      const url = new URL(location.href);
      url.hash = tagOf(release);
      try {
        await navigator.clipboard.writeText(url.href);
        window.clearTimeout(timeout);
        button.dataset.copied = 'true';
        if (label) label.textContent = '已复制';
        report(`${tagOf(release)} 的链接已复制`);
        timeout = window.setTimeout(() => {
          delete button.dataset.copied;
          if (label) label.textContent = original;
        }, 1800);
      } catch {
        setHash(release, true);
        report('可以从地址栏复制此版本链接');
      }
    });
  });

  const selected = fromHash(location.hash);
  if (selected) {
    product = selected.dataset.product || 'desktop';
    includePreview = selected.dataset.prerelease === 'true';
  }
  root.querySelectorAll<HTMLDetailsElement>('[data-release-details]').forEach(details => {
    details.open = !!selected && (details.contains(selected) || selected.contains(details));
  });
  document.documentElement.classList.add('changelog-enhanced');
  filter();
  if (selected) jump(selected, 'instant');

  if (!reducedMotion.matches) {
    void import('gsap').then(({ gsap }) => {
      if (reducedMotion.matches || !root!.isConnected) return;
      const intro = root!.querySelectorAll<HTMLElement>('[data-changelog-intro]');
      gsap.fromTo(intro, { y: 14, opacity: .3 }, { y: 0, opacity: 1, duration: .65, stagger: .07, ease: 'power3.out', clearProps: 'transform,opacity' });
      animateChange = () => {
        if (reducedMotion.matches) return;
        const first = visible()[0];
        if (!first) return;
        gsap.killTweensOf(first);
        gsap.fromTo(first, { opacity: .35, y: 8 }, { opacity: 1, y: 0, duration: .28, ease: 'power2.out', clearProps: 'transform,opacity' });
      };
    });
  }
}

const themeButton = document.querySelector<HTMLButtonElement>('[data-changelog-theme]');
function syncThemeControl() {
  const dark = document.documentElement.dataset.theme === 'dark';
  themeButton?.setAttribute('aria-pressed', String(dark));
  themeButton?.setAttribute('aria-label', dark ? '切换到浅色主题' : '切换到深色主题');
}
themeButton?.addEventListener('click', () => {
  const theme = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
  document.documentElement.dataset.theme = theme;
  try { localStorage.setItem('ncd-docs-theme', theme); } catch { /* Theme still works when storage is unavailable. */ }
  syncThemeControl();
});
syncThemeControl();
window.addEventListener('storage', event => {
  if (event.key !== 'ncd-docs-theme' || !['light', 'dark'].includes(event.newValue ?? '')) return;
  document.documentElement.dataset.theme = event.newValue!;
  syncThemeControl();
});
const menuButton = document.querySelector<HTMLButtonElement>('[data-changelog-menu]');
const nav = document.querySelector<HTMLElement>('[data-changelog-nav]');
menuButton?.addEventListener('click', () => {
  const open = menuButton.getAttribute('aria-expanded') !== 'true';
  menuButton.setAttribute('aria-expanded', String(open));
  nav?.classList.toggle('is-open', open);
});
document.addEventListener('keydown', event => {
  if (event.key !== 'Escape' || menuButton?.getAttribute('aria-expanded') !== 'true') return;
  menuButton.setAttribute('aria-expanded', 'false');
  nav?.classList.remove('is-open');
  menuButton.focus();
});

export {};
