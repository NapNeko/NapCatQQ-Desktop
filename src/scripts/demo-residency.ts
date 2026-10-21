type ResidencyState = { visible: boolean; protected: boolean };
type Entry = ResidencyState & { loaded: boolean; leftAt: number; evict: () => void };
type Clock = {
  now: () => number;
  setTimeout: (callback: () => void, delay: number) => number;
  clearTimeout: (timer: number) => void;
};

/** Keep automatic previews bounded without discarding an explicitly retained session. */
export class DemoResidency {
  private entries = new Set<Entry>();
  private timer: number | undefined;

  constructor(
    private clock: Clock,
    private limit = 2,
    private grace = 12_000,
    private onAvailable = () => {},
  ) {}

  register(evict: () => void) {
    const entry: Entry = { loaded: false, visible: false, protected: false, leftAt: this.clock.now(), evict };
    this.entries.add(entry);
    return {
      update: (state: ResidencyState) => {
        if ((entry.visible && !state.visible) || (entry.protected && !state.protected)) entry.leftAt = this.clock.now();
        const available = (entry.visible && !state.visible) || (!entry.protected && state.protected);
        Object.assign(entry, state);
        this.trim();
        this.schedule();
        if (available) this.onAvailable();
      },
      acquire: (explicit = false) => {
        if (entry.loaded) return true;
        if (!entry.protected) {
          while (this.automatic().length >= this.limit) {
            const candidate = this.offscreen()[0];
            if (!candidate) {
              // A direct button press wins over the background preview budget.
              if (!explicit) return false;
              break;
            }
            this.evict(candidate);
          }
        }
        entry.loaded = true;
        entry.leftAt = this.clock.now();
        this.schedule();
        return true;
      },
      release: () => {
        entry.loaded = false;
        this.schedule();
        this.onAvailable();
      },
    };
  }

  private automatic() {
    return [...this.entries].filter(entry => entry.loaded && !entry.protected);
  }

  private offscreen() {
    return this.automatic().filter(entry => !entry.visible).sort((a, b) => a.leftAt - b.leftAt);
  }

  private evict(entry: Entry) {
    entry.loaded = false;
    entry.evict();
    this.onAvailable();
  }

  private trim() {
    for (const entry of this.offscreen()) {
      if (this.automatic().length <= this.limit && this.clock.now() - entry.leftAt < this.grace) break;
      this.evict(entry);
    }
  }

  private schedule() {
    if (this.timer !== undefined) this.clock.clearTimeout(this.timer);
    const next = this.offscreen()[0];
    this.timer = next ? this.clock.setTimeout(() => {
      this.timer = undefined;
      this.trim();
      this.schedule();
    }, Math.max(0, this.grace - (this.clock.now() - next.leftAt))) : undefined;
  }
}
