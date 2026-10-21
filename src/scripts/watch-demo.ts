import gsap from 'gsap';

const stages = [
  ['在 Desktop 中管理', '先在远端安装 ncd-watch，配置检查和通知策略。'],
  ['关闭窗口，值守继续', 'SSH 管理连接断开；远端 Bot 和 ncd-watch 仍在运行。'],
  ['检查到 Bot 掉线', '模拟一次离线：ncd-watch 检查状态，触发已配置的通知规则。'],
  ['通知送达', '按配置向通知通道发送提醒，Desktop 无需重新打开。'],
];

document.querySelectorAll<HTMLElement>('[data-watch-demo]').forEach(root => {
  const get = <T extends HTMLElement = HTMLElement>(selector: string) => root.querySelector<T>(selector)!;
  const buttons = Array.from(root.querySelectorAll<HTMLButtonElement>('[data-watch-step]'));
  const play = get<HTMLButtonElement>('[data-watch-play]');
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const meter = get('.watch-meter');
  let visible = false;
  let started = false;
  let requested = false;

  function paint(step: number) {
    root.dataset.watchStage = String(step);
    get('[data-watch-count]').textContent = `0${step + 1} / 04`;
    get('[data-watch-title]').textContent = stages[step][0];
    get('[data-watch-description]').textContent = stages[step][1];
    get('[data-watch-desktop]').textContent = step > 0 ? '已退出' : '已连接';
    get('[data-watch-link]').textContent = step > 0 ? '已断开' : 'SSH';
    get('[data-watch-bot]').textContent = step > 1 ? '已离线' : '运行中';
    get('[data-watch-notice-title]').textContent = step === 3 ? 'Bot 10003 已离线' : step === 2 ? '检测到异常' : '通知通道已就绪';
    get('[data-watch-notice-copy]').textContent = step === 3 ? '掉线提醒 · 已发送' : step === 2 ? '准备发送掉线提醒' : '等待检查结果';
    buttons.forEach((button, index) => button.setAttribute('aria-pressed', String(index === step)));
  }

  function label() {
    play.setAttribute('aria-pressed', String(requested));
    get('[data-watch-play-label]').textContent = reduced.matches ? '下一步' : requested ? '暂停' : timeline.progress() === 1 ? '重新演示' : started ? '继续演示' : '演示流程';
    get('[data-watch-play-icon]').textContent = requested ? 'Ⅱ' : '▶';
  }

  const timeline = gsap.timeline({ paused: true, onUpdate: () => {
    meter.setAttribute('aria-valuenow', String(Math.round(timeline.progress() * 100)));
  }, onComplete: () => { requested = false; sync(); } });
  timeline.fromTo(get('.watch-meter > i'), { scaleX: 0 }, { scaleX: 1, duration: 16, ease: 'none' }, 0);
  stages.forEach((_, index) => timeline.call(() => paint(index), [], index * 4));
  timeline.fromTo(get('.watch-packet'), { left: '0%' }, { left: '100%', duration: 1.6, repeat: 1, ease: 'none' }, 0);
  timeline.fromTo(get('.watch-scan > i'), { scaleX: .05 }, { scaleX: 1, duration: 3.6, repeat: 3, ease: 'none' }, 0);
  timeline.fromTo(get('.watch-radar > i'), { scale: .35, opacity: .8 }, { scale: 1.8, opacity: 0, duration: 1.8, repeat: 7, ease: 'power1.out' }, 0);
  timeline.fromTo(get('.watch-notice-route > i'), { top: '0%' }, { top: '100%', duration: 1, ease: 'power1.inOut' }, 10.8);
  timeline.fromTo(get('.watch-notice'), { y: 0 }, { y: -5, duration: .4, repeat: 1, yoyo: true, ease: 'power2.out' }, 12);
  paint(0);

  function sync() {
    const playing = visible && !document.hidden && requested;
    root.dataset.playing = String(playing);
    meter.setAttribute('aria-valuenow', String(Math.round(timeline.progress() * 100)));
    if (playing) timeline.play(); else timeline.pause();
    label();
  }

  play.addEventListener('click', () => {
    if (reduced.matches) {
      const step = (Number(root.dataset.watchStage) + 1) % stages.length;
      timeline.pause(step * 4, true); paint(step); started = true; sync();
      return;
    }
    if (timeline.progress() === 1) { timeline.pause(0, true); paint(0); }
    started = true; requested = !requested; sync();
  });
  buttons.forEach((button, step) => button.addEventListener('click', () => {
    started = true; requested = false;
    timeline.pause(step * 4, true); paint(step); sync();
  }));
  const observer = new IntersectionObserver(entries => {
    visible = entries[0].isIntersecting && entries[0].intersectionRatio >= .35;
    if (visible && !started && !reduced.matches) { started = true; requested = true; }
    sync();
  }, { threshold: [0, .35] });
  observer.observe(root);
  document.addEventListener('visibilitychange', sync);
  const changeMotion = () => { if (reduced.matches) requested = false; sync(); };
  reduced.addEventListener('change', changeMotion);
  label();
  const dispose = () => { timeline.kill(); observer.disconnect(); document.removeEventListener('visibilitychange', sync); reduced.removeEventListener('change', changeMotion); };
  window.addEventListener('pagehide', event => { if (!event.persisted) dispose(); });
  if (import.meta.hot) import.meta.hot.dispose(dispose);
});
