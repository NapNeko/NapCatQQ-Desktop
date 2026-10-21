import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

gsap.registerPlugin(ScrollTrigger);

function mountHomeMotion() {
  const root = document.querySelector<HTMLElement>('.home-page');
  if (!root) return () => {};
  const media = gsap.matchMedia();

  media.add('(prefers-reduced-motion: no-preference)', () => {
    const intro = gsap.timeline({ defaults: { ease: 'power3.out' } });
    intro.from('.hero-word', { yPercent: 110, rotation: 5, duration: 1.15, stagger: .13, clearProps: 'transform' })
      .from('[data-hero-reveal]', { y: 26, opacity: 0, duration: .9, stagger: .1, clearProps: 'transform,opacity' }, .2)
      .from('.hero-footnote', { opacity: 0, duration: .7, clearProps: 'opacity' }, .7);

    const reveals = new Map<HTMLElement, gsap.core.Tween>();
    const revealSelectors = [
      '.section-heading', '.management-workspace', '.start-path',
      '.framework-selector', '.apps-product-layout', '.apps-lifecycle',
      '.chat-feature-selector', '.chat-detail-row', '.chat-product-figure',
      '.chat-account-note', '.ops-tool-tabs', '.ops-watch-demo',
      '.ops-watch-features', '.appearance-demo', '.everyday-details article',
      '.download-panel', '.footer-main',
    ].join(',');
    gsap.utils.toArray<HTMLElement>(revealSelectors, root).forEach(element => {
      const tween = gsap.from(element, {
        y: element.matches('[role="tablist"], [role="group"]') ? 0 : 38,
        opacity: 0, duration: .85, ease: 'power3.out',
        clearProps: 'transform,opacity',
        scrollTrigger: { trigger: element, start: 'top 92%', once: true },
      });
      reveals.set(element, tween);
    });

    gsap.to('.reading-progress', {
      scaleX: 1, ease: 'none',
      scrollTrigger: { trigger: root, start: 'top top', end: 'bottom bottom', scrub: true },
    });

    // 聚焦操作入口时立刻完成其入场，避免键盘焦点落在透明内容里。
    const revealFocus = (event: FocusEvent) => {
      if (!(event.target instanceof Element) || !event.target.matches(':focus-visible')) return;
      for (const [element, tween] of reveals) {
        if (element.contains(event.target)) tween.progress(1);
      }
      if (event.target.closest('.hero')) intro.progress(1);
    };
    root.addEventListener('focusin', revealFocus);
    return () => root.removeEventListener('focusin', revealFocus);
  });

  media.add('(min-width: 901px) and (prefers-reduced-motion: no-preference)', () => {
    gsap.to('.workspace-screen', {
      y: -45, rotation: 2, scale: 1.04, ease: 'none',
      scrollTrigger: { trigger: '.hero', start: 'top top', end: 'bottom top', scrub: .8 },
    });
    gsap.to('.terminal-sculpture', {
      y: 80, rotation: 4, ease: 'none',
      scrollTrigger: { trigger: '.hero', start: 'top top', end: 'bottom top', scrub: 1 },
    });
    gsap.to('.hero-art-orbit', {
      rotation: 35, ease: 'none',
      scrollTrigger: { trigger: '.hero', start: 'top top', end: 'bottom top', scrub: 1 },
    });
  });

  // 桌面上的横向叙事只占一段滚动。矮屏、触屏和减少动态效果模式使用完整静态布局。
  media.add('(min-width: 1101px) and (min-height: 760px) and (pointer: fine) and (prefers-reduced-motion: no-preference)', () => {
    const story = root.querySelector<HTMLElement>('.workflow-story')!;
    const stage = story.querySelector<HTMLElement>('.story-stage')!;
    const viewport = story.querySelector<HTMLElement>('.story-viewport')!;
    const track = story.querySelector<HTMLElement>('.story-track')!;
    const cards = [...track.querySelectorAll<HTMLElement>('.story-card')];
    const current = story.querySelector<HTMLElement>('[data-story-current]')!;
    const progress = story.querySelector<HTMLElement>('.story-progress > span')!;
    story.classList.add('story-pinned');
    const distance = () => Math.max(0, track.scrollWidth - viewport.clientWidth);
    const timeline = gsap.timeline({
      scrollTrigger: {
        trigger: story,
        start: 'top 77px',
        end: () => `+=${distance()}`,
        pin: stage,
        scrub: .7,
        anticipatePin: 1,
        invalidateOnRefresh: true,
        onUpdate(self) {
          const offset = self.progress * distance();
          const index = cards.reduce((selected, card, i) =>
            offset + viewport.clientWidth * .5 >= card.offsetLeft ? i : selected, 0);
          current.textContent = String(index + 1).padStart(2, '0');
        },
      },
    });
    timeline.to(track, { x: () => -distance(), ease: 'none' }, 0)
      .to(progress, { scaleX: 1, ease: 'none' }, 0)
      .to('.shape-bloom', { rotation: 100, ease: 'none' }, 0)
      .to('.shape-orbit', { rotation: -30, ease: 'none' }, 0);

    // 屏外的文档链接仍可 Tab 到达；同步滚动以把对应卡片带回视口。
    const focusCard = (event: FocusEvent) => {
      if (!(event.target instanceof Element) || !event.target.matches(':focus-visible')) return;
      const card = event.target.closest<HTMLElement>('.story-card');
      const trigger = timeline.scrollTrigger;
      if (!card || !trigger) return;
      const target = Math.min(card.offsetLeft, distance());
      const fraction = distance() ? target / distance() : 0;
      window.scrollTo({ top: trigger.start + fraction * (trigger.end - trigger.start), behavior: 'instant' });
      timeline.progress(fraction);
    };
    story.addEventListener('focusin', focusCard);
    return () => {
      story.removeEventListener('focusin', focusCard);
      story.classList.remove('story-pinned');
      current.textContent = '01';
    };
  });

  // 字体、图片和展开的 FAQ 会改变页面高度，合并刷新避免逐帧测量布局。
  let refreshFrame = 0;
  let disposed = false;
  const refresh = () => {
    if (disposed) return;
    cancelAnimationFrame(refreshFrame);
    refreshFrame = requestAnimationFrame(() => ScrollTrigger.refresh());
  };
  const observer = new ResizeObserver(refresh);
  const main = root.querySelector('main');
  if (main) observer.observe(main);
  void document.fonts.ready.then(refresh);
  window.addEventListener('load', refresh, { once: true });
  refresh();

  return () => {
    disposed = true;
    observer.disconnect();
    cancelAnimationFrame(refreshFrame);
    window.removeEventListener('load', refresh);
    media.revert();
  };
}

let unmount = mountHomeMotion();
const onPageHide = () => unmount();
const onPageShow = (event: PageTransitionEvent) => {
  if (event.persisted) unmount = mountHomeMotion();
};
window.addEventListener('pagehide', onPageHide);
window.addEventListener('pageshow', onPageShow);
if (import.meta.hot) import.meta.hot.dispose(() => {
  unmount();
  window.removeEventListener('pagehide', onPageHide);
  window.removeEventListener('pageshow', onPageShow);
});
