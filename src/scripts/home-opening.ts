import gsap from 'gsap';

export function mountHomeOpening(root: HTMLElement) {
  const overlay = root.querySelector<HTMLElement>('[data-home-opening]')!;
  const logo = overlay.querySelector<HTMLElement>('.opening-logo')!;
  const skip = overlay.querySelector<HTMLButtonElement>('[data-skip-opening]')!;
  const replay = root.querySelector<HTMLButtonElement>('[data-replay-opening]')!;
  const target = root.querySelector<SVGElement>('.cat-layer-front')!;
  const media = matchMedia('(prefers-reduced-motion: reduce)');
  let animation: gsap.Context | undefined;
  let timeline: gsap.core.Timeline | undefined;
  let timeout = 0;
  let active = false;
  let inert: Array<{ element: HTMLElement; previous: boolean }> = [];

  function finish() {
    const restoreFocus = overlay.contains(document.activeElement);
    active = false;
    window.clearTimeout(timeout);
    timeline?.progress(1, true);
    animation?.revert();
    animation = undefined;
    timeline = undefined;
    overlay.hidden = true;
    document.documentElement.dataset.homeIntro = 'ready';
    inert.forEach(({ element, previous }) => { element.inert = previous; });
    inert = [];
    if (restoreFocus) replay.focus({ preventScroll: true });
    document.dispatchEvent(new CustomEvent('home:intro-ready'));
  }

  function play() {
    if (active) finish();
    if (media.matches) { finish(); return; }
    window.scrollTo({ top: 0, behavior: 'instant' });
    document.documentElement.dataset.homeIntro = 'playing';
    document.dispatchEvent(new CustomEvent('home:intro-start'));
    overlay.hidden = false;
    active = true;
    inert = Array.from(root.children).filter((element): element is HTMLElement =>
      element instanceof HTMLElement && element !== overlay && !element.matches('script, style, link'))
      .map(element => ({ element, previous: element.inert }));
    inert.forEach(({ element }) => { element.inert = true; });
    timeout = window.setTimeout(finish, 5500);
    animation = gsap.context(() => {
      const destination = target.getBoundingClientRect();
      const origin = logo.getBoundingClientRect();
      const x = destination.left + destination.width / 2 - (origin.left + origin.width / 2);
      const y = destination.top + destination.height / 2 - (origin.top + origin.height / 2);
      timeline = gsap.timeline({ defaults: { ease: 'power3.inOut' }, onComplete: finish });
      timeline.from('.opening-grid', { scale: 1.3, opacity: 0, duration: 1.2 }, 0)
        .fromTo('.opening-orbit circle:not(.opening-orbit-point), .opening-orbit path', { strokeDasharray: 1, strokeDashoffset: 1 }, { strokeDashoffset: 0, duration: 1.4, stagger: .12 }, 0)
        .from('.opening-type > span', { yPercent: 75, rotation: 6, opacity: 0, duration: 1.05, stagger: .12, ease: 'power4.out' }, .1)
        .from(logo, { scale: .55, rotationX: 28, rotationY: -35, rotationZ: -24, opacity: 0, duration: 1.15, ease: 'power3.out' }, .2)
        .from('.opening-layer-back', { x: 120, y: -110, rotation: 35, opacity: 0, duration: 1.1 }, .15)
        .from('.opening-layer-middle', { x: -100, y: 90, rotation: -25, opacity: 0, duration: 1 }, .3)
        .fromTo('.opening-prompt', { strokeDasharray: 1, strokeDashoffset: 1 }, { strokeDashoffset: 0, duration: .6 }, .85)
        .from('.opening-labels > span', { y: 12, opacity: 0, duration: .5, stagger: .12 }, .7)
        .to('.opening-orbit-point', { rotation: 235, duration: 2.2, ease: 'power2.inOut' }, 0)
        .fromTo('.opening-window', { scaleX: .3, scaleY: .015, opacity: 0 }, { scaleX: 1, scaleY: 1, opacity: .8, duration: 1.1 }, 1.05)
        .to('.opening-type > span', { yPercent: -35, opacity: 0, duration: .65, stagger: .07 }, 1.7)
        .to('.opening-grid, .opening-orbit, .opening-labels, .opening-registration, .opening-edition, .opening-skip', { opacity: 0, duration: .7 }, 1.85)
        .to('.opening-window', { scale: 1.35, opacity: 0, duration: .9 }, 1.95)
        .to('.opening-paper', { opacity: 0, duration: 1.25, ease: 'power2.inOut' }, 1.9)
        .to(logo, { x, y, scale: destination.width / origin.width, rotationX: 12, rotationY: -22, rotationZ: 14, duration: 1.3 }, 1.85)
        .to(logo, { opacity: 0, duration: .45 }, 2.75)
        .fromTo(root.querySelector('.cat-art'), { opacity: 0 }, { opacity: 1, duration: .55 }, 2.65)
        .fromTo(root.querySelector('.workspace-preview'), { clipPath: 'inset(48% 0% 48% 0% round 12px)', opacity: 0, y: 32 }, { clipPath: 'inset(0% 0% 0% 0% round 12px)', opacity: 1, y: 0, duration: 1.1 }, 2.15);
    }, root);
  }

  const skipAnimation = () => {
    if (!active) return;
    document.dispatchEvent(new CustomEvent('home:intro-skip'));
    finish();
  };
  const keydown = (event: KeyboardEvent) => { if (event.key === 'Escape') skipAnimation(); };
  const resize = () => { if (active) skipAnimation(); };
  const changeMotion = () => { if (media.matches) skipAnimation(); };
  skip.addEventListener('click', skipAnimation);
  replay.addEventListener('click', play);
  window.addEventListener('wheel', skipAnimation, { passive: true });
  window.addEventListener('touchmove', skipAnimation, { passive: true });
  window.addEventListener('resize', resize);
  document.addEventListener('keydown', keydown);
  media.addEventListener('change', changeMotion);
  if (document.documentElement.dataset.homeIntro === 'pending' && window.scrollY < 64) play();
  else finish();

  return () => {
    finish();
    skip.removeEventListener('click', skipAnimation);
    replay.removeEventListener('click', play);
    window.removeEventListener('wheel', skipAnimation);
    window.removeEventListener('touchmove', skipAnimation);
    window.removeEventListener('resize', resize);
    document.removeEventListener('keydown', keydown);
    media.removeEventListener('change', changeMotion);
  };
}
