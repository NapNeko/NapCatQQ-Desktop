import type { AppRoute } from '@desktop/shared/components/next/Sidebar';

export type DemoTarget = () => HTMLElement | null;
export type DemoStep = readonly [title: string, detail: string];

export type DemoDriver = {
  by(selector: string): DemoTarget;
  button(text: string): DemoTarget;
  input(label: string): DemoTarget;
  text(text: string, selector?: string): DemoTarget;
  click(target: DemoTarget): Promise<void>;
  select(trigger: DemoTarget, option: DemoTarget, selected: () => boolean): Promise<void>;
  type(target: DemoTarget, value: string): Promise<void>;
  point(target: DemoTarget): Promise<void>;
  find(target: DemoTarget): Promise<HTMLElement>;
  wait(milliseconds: number): Promise<void>;
  until(check: () => boolean, message?: string): Promise<void>;
};

export type DemoLesson = {
  id: string;
  route: AppRoute;
  steps: readonly DemoStep[];
  reset(): void;
  actions: ReadonlyArray<(driver: DemoDriver) => Promise<void>>;
  observe?(): number;
};
