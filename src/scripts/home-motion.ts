import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { mountHomeOpening } from './home-opening';
import { observeLayoutRefresh } from './layout-refresh';

gsap.registerPlugin(ScrollTrigger);

function mountHomeMotion() {
  const root = document.querySelector<HTMLElement>('.home-page');
  if (!root) return () => {};
  const media = gsap.matchMedia();
  const select = gsap.utils.selector(root);

  media.add({
    motion: '(prefers-reduced-motion: no-preference)',
    desktop: '(min-width: 1050px) and (min-height: 740px) and (pointer: fine)',
    reduced: '(prefers-reduced-motion: reduce)',
  }, context => {
    if (context.conditions?.reduced) return;
    const desktop = Boolean(context.conditions?.desktop);
    const cleanups: Array<() => void> = [];
    const hero = root.querySelector<HTMLElement>('.hero')!;
    const scene = hero.querySelector<HTMLElement>('.hero-scene')!;
    const preview = hero.querySelector<HTMLElement>('.hero-preview-wrap')!;
    const art = hero.querySelector<HTMLElement>('.cat-art')!;

    const intro = gsap.timeline({ delay: document.documentElement.dataset.homeIntro === 'pending' ? 2 : 0, defaults: { ease: 'power3.out' } });
    intro.from(select('.hero-word'), { yPercent: 115, rotation: 5, duration: 1.1, stagger: .14, clearProps: 'transform' })
      .from(select('[data-hero-reveal]'), { y: 18, opacity: 0, duration: .8, stagger: .12, clearProps: 'transform,opacity' }, .3)
      .from(select('.cat-object'), { y: 70, rotationZ: -12, opacity: 0, duration: 1.4 }, .12)
      .fromTo(select('.cat-prompt'), { strokeDasharray: 1, strokeDashoffset: 1 }, { strokeDashoffset: 0, duration: .8, ease: 'power2.inOut' }, .55);
    const restartIntro = () => { ScrollTrigger.update(); intro.delay(2).restart(true); };
    const finishIntro = () => { intro.progress(1); };
    document.addEventListener('home:intro-start', restartIntro);
    document.addEventListener('home:intro-skip', finishIntro);
    cleanups.push(() => {
      document.removeEventListener('home:intro-start', restartIntro);
      document.removeEventListener('home:intro-skip', finishIntro);
    });

    gsap.fromTo(select('.hero-circuit path'), { strokeDasharray: 1, strokeDashoffset: 1 }, {
      strokeDashoffset: 0, stagger: .08, ease: 'none',
      scrollTrigger: { trigger: hero, start: 'top 65%', end: 'bottom 30%', scrub: .8 },
    });
    gsap.to(select('.reading-progress'), {
      scaleX: 1, ease: 'none',
      scrollTrigger: { trigger: root, start: 'top top', end: 'bottom bottom', scrub: true },
    });

    const reveals = new Map<HTMLElement, gsap.core.Tween>();
    gsap.utils.toArray<HTMLElement>('.section-heading h2', root).forEach(heading => {
      const tween = gsap.from(heading, {
        clipPath: 'inset(0 0 100% 0)', yPercent: 25, duration: .9, ease: 'power3.out',
        clearProps: 'clipPath,transform',
        scrollTrigger: { trigger: heading, start: 'top 90%', once: true },
      });
      reveals.set(heading, tween);
    });
    const visuals = '.management-detail, .apps-product-figure, .chat-product-figure, .home-tool-demo, .watch-demo, .home-settings-demo';
    gsap.utils.toArray<HTMLElement>(visuals, root).forEach(visual => {
      if (visual.closest('[hidden]')) return;
      const live = Boolean(visual.querySelector('iframe'));
      const tween = gsap.fromTo(visual, {
        y: desktop ? 65 : 24, scale: desktop ? .94 : .98,
        ...(live ? {} : { rotationX: desktop ? 8 : 0, transformPerspective: 1200 }),
      }, {
        y: 0, scale: 1, ...(live ? {} : { rotationX: 0 }), ease: 'none',
        scrollTrigger: { trigger: visual, start: 'top 95%', end: 'top 42%', scrub: .65 },
      });
      reveals.set(visual, tween);
    });
    gsap.utils.toArray<HTMLElement>('.feature-art', root).forEach(artwork => {
      const paths = artwork.querySelectorAll('[data-draw]');
      const pieces = artwork.querySelectorAll('[data-art-piece]');
      const sequence = gsap.timeline({
        scrollTrigger: { trigger: artwork, start: 'top 92%', end: 'bottom 48%', scrub: .8 },
      });
      if (paths.length) sequence.fromTo(paths, { strokeDasharray: 1, strokeDashoffset: 1 }, {
        strokeDashoffset: 0, duration: 1, stagger: .1, ease: 'none',
      }, 0);
      if (pieces.length) sequence.from(pieces, {
        y: desktop ? 45 : 20, x: i => (i % 2 ? 1 : -1) * (desktop ? 22 : 8),
        rotation: i => (i % 2 ? 1 : -1) * 8, scale: .88,
        duration: 1, stagger: .1, ease: 'power2.out',
      }, 0);
    });
    gsap.utils.toArray<HTMLElement>('.chapter-ghost', root).forEach(number => {
      gsap.fromTo(number, { y: 35 }, {
        y: -65, ease: 'none',
        scrollTrigger: { trigger: number.closest('section'), start: 'top bottom', end: 'bottom top', scrub: 1 },
      });
    });
    gsap.fromTo(select('.feature-art-download'), { rotation: -24, y: 60 }, {
      rotation: -8, y: -20, ease: 'none',
      scrollTrigger: { trigger: '.get-started-section', start: 'top bottom', end: 'bottom bottom', scrub: .8 },
    });
    gsap.from(select('.footer-wordmark'), {
      yPercent: 55, ease: 'none',
      scrollTrigger: { trigger: '.site-footer', start: 'top bottom', end: 'bottom bottom', scrub: .7 },
    });

    const revealFocus = (event: FocusEvent) => {
      if (!(event.target instanceof Element) || !event.target.matches(':focus-visible')) return;
      if (event.target.closest('.hero')) intro.progress(1);
      for (const [element, tween] of reveals) {
        if (element.contains(event.target)) tween.progress(1);
      }
    };
    root.addEventListener('focusin', revealFocus);
    cleanups.push(() => root.removeEventListener('focusin', revealFocus));

    if (desktop) {
      hero.classList.add('hero-cinematic');
      const heroTimeline = gsap.timeline({
        defaults: { ease: 'power2.inOut' },
        scrollTrigger: {
          trigger: hero, start: () => 'top ' + getComputedStyle(root).getPropertyValue('--header-height').trim(),
          // 首屏用 CSS sticky；ScrollTrigger 的 pin 重挂 DOM 会让真实 iframe 重新加载。
          end: () => '+=' + Math.round(innerHeight * 1.15), scrub: .8,
          invalidateOnRefresh: true,
        },
      });
      heroTimeline.fromTo(preview, {
        x: () => Math.min(innerWidth * .18, 280), y: () => Math.min(innerHeight * .13, 120),
        yPercent: -50, scale: .64, rotationZ: 5,
      }, { x: 0, y: 0, yPercent: -50, scale: 1, rotationX: 0, rotationY: 0, rotationZ: 0, duration: 1 }, .08)
        .to(select('.hero-signature'), { y: -55, x: -30, autoAlpha: 0, duration: .4, ease: 'power1.in' }, 0)
        .to(art, { x: 180, y: -140, rotation: 24, scale: 1.2, autoAlpha: 0, duration: .72 }, .05)
        .to(select('.cat-layer-back'), { x: -130, y: 120, rotation: -18, duration: .65 }, 0)
        .to(select('.cat-layer-middle'), { x: 75, y: -60, rotation: 14, duration: .65 }, 0)
        .fromTo(select('.hero-preview-controls'), { opacity: .65 }, { opacity: 1, duration: .4 }, .48)
        .to(select('.hero-grid'), { opacity: .16, duration: .8 }, 0)
        .to(select('.hero-tint'), { opacity: 1, duration: .6 }, .2)
        .to(select('.hero-scroll-track i'), { scaleX: 1, duration: 1.08, ease: 'none' }, 0);

      // Tab 进入产品窗口时完成镜头推进，焦点始终留在可读画面内。
      const focusHero = (event: FocusEvent) => {
        if (!(event.target instanceof Element) || !event.target.matches(':focus-visible')) return;
        const trigger = heroTimeline.scrollTrigger!;
        if (preview.contains(event.target)) {
          window.scrollTo({ top: trigger.end, behavior: 'instant' });
          heroTimeline.progress(1);
        } else if (event.target.closest('.hero-signature')) {
          window.scrollTo({ top: trigger.start, behavior: 'instant' });
          heroTimeline.progress(0);
        }
      };
      hero.addEventListener('focusin', focusHero);
      const focusWorkspace = () => {
        intro.progress(1);
        window.scrollTo({ top: heroTimeline.scrollTrigger!.end, behavior: 'instant' });
        heroTimeline.progress(1);
      };
      const resetHero = () => { heroTimeline.progress(0); };
      hero.addEventListener('tutorial:interact', focusWorkspace);
      document.addEventListener('home:intro-start', resetHero);
      cleanups.push(() => {
        hero.removeEventListener('focusin', focusHero);
        hero.removeEventListener('tutorial:interact', focusWorkspace);
        document.removeEventListener('home:intro-start', resetHero);
        hero.classList.remove('hero-cinematic');
      });

      const object = art.querySelector<HTMLElement>('.cat-object')!;
      const rotateX = gsap.quickTo(object, 'rotationX', { duration: .8, ease: 'power3.out' });
      const rotateY = gsap.quickTo(object, 'rotationY', { duration: .8, ease: 'power3.out' });
      const move = (event: PointerEvent) => {
        if (intro.isActive() || heroTimeline.progress() > .45) return;
        const bounds = scene.getBoundingClientRect();
        rotateX(12 - (event.clientY - bounds.top - bounds.height / 2) / bounds.height * 14);
        rotateY(-22 + (event.clientX - bounds.left - bounds.width / 2) / bounds.width * 18);
      };
      const reset = () => { rotateX(12); rotateY(-22); };
      scene.addEventListener('pointermove', move);
      scene.addEventListener('pointerleave', reset);
      cleanups.push(() => {
        scene.removeEventListener('pointermove', move);
        scene.removeEventListener('pointerleave', reset);
      });

      const story = root.querySelector<HTMLElement>('.workflow-story')!;
      const stage = story.querySelector<HTMLElement>('.story-stage')!;
      const viewport = story.querySelector<HTMLElement>('.story-viewport')!;
      const track = story.querySelector<HTMLElement>('.story-track')!;
      const panels = [...track.querySelectorAll<HTMLElement>('.story-panel')];
      const links = [...story.querySelectorAll<HTMLAnchorElement>('[data-story-jump]')];
      const progress = story.querySelector<HTMLElement>('.story-progress > span')!;
      story.classList.add('story-pinned');
      const distance = () => Math.max(0, track.scrollWidth - viewport.clientWidth);
      let activePanel = -1;
      const timeline = gsap.timeline({
        scrollTrigger: {
          trigger: story, start: () => 'top ' + getComputedStyle(root).getPropertyValue('--header-height').trim(),
          end: () => '+=' + Math.round(distance() * .95), pin: stage, scrub: .75,
          anticipatePin: 1, invalidateOnRefresh: true,
          onUpdate(self) {
            const index = Math.round(self.progress * (panels.length - 1));
            if (index === activePanel) return;
            activePanel = index;
            links.forEach((link, i) => {
              if (i === index) link.setAttribute('aria-current', 'step');
              else link.removeAttribute('aria-current');
            });
          },
        },
      });
      timeline.to(track, { x: () => -distance(), duration: 2, ease: 'none' }, 0)
        .to(progress, { scaleX: 1, duration: 2, ease: 'none' }, 0);
      panels.forEach((panel, i) => {
        const at = Math.max(0, i - .65);
        timeline.fromTo(panel.querySelector('figure'), { y: 60, rotation: i % 2 ? 8 : -8, scale: .9 }, {
          y: 0, rotation: 0, scale: 1, duration: .65, ease: 'power2.out',
        }, at);
        timeline.fromTo(panel.querySelector('.story-wire path'), { strokeDasharray: 1, strokeDashoffset: 1 }, {
          strokeDashoffset: 0, duration: .65, ease: 'none',
        }, at);
        timeline.fromTo(panel.querySelector('.story-watermark'), { x: 90 }, { x: 0, duration: .65, ease: 'none' }, at);
      });
      const goToPanel = (index: number, smooth: boolean) => {
        const trigger = timeline.scrollTrigger!;
        const fraction = index / (panels.length - 1);
        window.scrollTo({ top: trigger.start + fraction * (trigger.end - trigger.start), behavior: smooth ? 'smooth' : 'instant' });
        if (!smooth) timeline.progress(fraction);
      };
      const jump = (event: MouseEvent) => {
        const link = (event.target as Element).closest<HTMLAnchorElement>('[data-story-jump]');
        if (!link || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
        event.preventDefault();
        goToPanel(Number(link.dataset.storyJump), true);
      };
      const focusPanel = (event: FocusEvent) => {
        if (!(event.target instanceof Element) || !event.target.matches(':focus-visible')) return;
        const panel = event.target.closest<HTMLElement>('.story-panel');
        if (panel) goToPanel(panels.indexOf(panel), false);
      };
      story.addEventListener('click', jump);
      story.addEventListener('focusin', focusPanel);
      cleanups.push(() => {
        story.removeEventListener('click', jump);
        story.removeEventListener('focusin', focusPanel);
        story.classList.remove('story-pinned');
        links.forEach((link, i) => i ? link.removeAttribute('aria-current') : link.setAttribute('aria-current', 'step'));
      });
    } else {
      // 小屏保留景深和路径变化，纵向阅读不被固定场景截住。
      gsap.fromTo(preview, { y: 24, scale: .96 }, {
        y: 0, scale: 1, ease: 'none',
        scrollTrigger: { trigger: preview, start: 'top bottom', end: 'top 45%', scrub: .5 },
      });
      gsap.to(art, {
        y: -35, rotation: -9, ease: 'none',
        scrollTrigger: { trigger: hero, start: 'top top', end: 'bottom top', scrub: .7 },
      });
      gsap.utils.toArray<HTMLElement>('.story-panel', root).forEach(panel => {
        gsap.from(panel.querySelector('figure'), {
          y: 35, rotation: 0, scale: .94, ease: 'none',
          scrollTrigger: { trigger: panel, start: 'top 85%', end: 'center 60%', scrub: .6 },
        });
        gsap.fromTo(panel.querySelector('.story-wire path'), { strokeDasharray: 1, strokeDashoffset: 1 }, {
          strokeDashoffset: 0, ease: 'none',
          scrollTrigger: { trigger: panel, start: 'top 80%', end: 'bottom 50%', scrub: .6 },
        });
      });
    }
    return () => cleanups.forEach(cleanup => cleanup());
  });

  const stopOpening = mountHomeOpening(root);

  const layout = observeLayoutRefresh(root.querySelector('main'), () => ScrollTrigger.refresh(true));
  void document.fonts.ready.then(layout.request);
  window.addEventListener('load', layout.request, { once: true });
  layout.request();

  return () => {
    layout.dispose();
    window.removeEventListener('load', layout.request);
    stopOpening();
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
