type Listener = (active: boolean) => void;

const listeners = new Set<Listener>();
let active = false;
let idleTimer: number | undefined;

function notify(value: boolean) {
  if (active === value) return;
  active = value;
  for (const listener of [...listeners]) listener(active);
}

function onScroll() {
  window.clearTimeout(idleTimer);
  notify(true);
  if (!listeners.size) return;
  idleTimer = window.setTimeout(() => {
    idleTimer = undefined;
    notify(false);
  }, 160);
}

export function isScrollActive() {
  return active;
}

export function subscribeScrollActivity(listener: Listener) {
  const subscriber: Listener = value => listener(value);
  if (!listeners.size) window.addEventListener('scroll', onScroll, { passive: true });
  listeners.add(subscriber);
  subscriber(active);
  return () => {
    if (!listeners.delete(subscriber) || listeners.size) return;
    window.removeEventListener('scroll', onScroll);
    window.clearTimeout(idleTimer);
    idleTimer = undefined;
    active = false;
  };
}
