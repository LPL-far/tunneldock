import { act, StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { AgentActivityPanel } from './AgentActivityPanel';
import { I18nProvider } from '../../i18n';
import type { ProjectActivity } from '../../types/activity';

// Browser-only helper: real panels and React DOM, no backend or application state.
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
const container = document.createElement('div');
document.body.append(container);
const root = createRoot(container);
const intervals = new Map<number, () => void>();
let nextInterval = 0;
let now = Date.parse('2026-10-03T12:00:00Z');
const original = {
  setInterval: window.setInterval, clearInterval: window.clearInterval,
  now: Date.now, error: console.error,
  add: EventTarget.prototype.addEventListener,
  remove: EventTarget.prototype.removeEventListener,
};
const errors: string[] = [];
const listeners: { target: EventTarget; type: string; callback: unknown; capture: boolean }[] = [];
const captureOf = (options?: boolean | AddEventListenerOptions | EventListenerOptions) =>
  typeof options === 'boolean' ? options : !!options?.capture;
EventTarget.prototype.addEventListener = function (type, callback, options) {
  if ((this === window || this === document) && !listeners.some(l =>
    l.target === this && l.type === type && l.callback === callback && l.capture === captureOf(options))) {
    listeners.push({ target: this, type, callback, capture: captureOf(options) });
  }
  original.add.call(this, type, callback, options);
};
EventTarget.prototype.removeEventListener = function (type, callback, options) {
  const index = listeners.findIndex(l => l.target === this && l.type === type &&
    l.callback === callback && l.capture === captureOf(options));
  if (index >= 0) listeners.splice(index, 1);
  original.remove.call(this, type, callback, options);
};
window.setInterval = ((callback: () => void, delay: number) => {
  if (delay !== 1000) throw new Error(`Unexpected interval: ${delay}`);
  const id = ++nextInterval;
  intervals.set(id, callback);
  return id;
}) as typeof window.setInterval;
window.clearInterval = (id) => { if (typeof id === 'number') intervals.delete(id); };
Date.now = () => now;
console.error = (...args) => { errors.push(args.map(String).join(' ')); };

function check(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}
function snapshot(projectId: string, revision = 0): ProjectActivity {
  return {
    version: 1, project_id: projectId, revision: String(revision), sampled_at: new Date(now).toISOString(),
    cards: [], pending_reviews: [], pending_review_count: 0,
    campaigns: { items: [], total: 0, revision, room_enabled: true },
    context_engine: { phase: 'ready', checked_at: new Date(now).toISOString(), revision: String(revision) },
  };
}
function render(data: ProjectActivity | null, error: string | null = null) {
  act(() => root.render(<StrictMode><I18nProvider initialLocale="en-US">
    <AgentActivityPanel data={data} error={error} cwd="D:/isolated-fixture"/>
  </I18nProvider></StrictMode>));
}
function panels(expected: number) {
  check(container.querySelectorAll('section[aria-label="Research campaigns"]').length === expected,
    `Expected ${expected} campaign panels, got ${container.querySelectorAll('section[aria-label="Research campaigns"]').length}`);
  check(container.querySelectorAll('section[aria-label="Context and automatic memory packing"]').length === expected,
    `Expected ${expected} context panels`);
  check((container.textContent?.match(/No recorded campaigns\./g) ?? []).length === expected,
    'Empty campaign messages must not accumulate');
  check(intervals.size === 1, `Expected one activity clock, got ${intervals.size}`);
  check(listeners.length === 0, 'Panel leaked a global listener');
}
function fields() {
  const draft = container.querySelector('textarea');
  const query = container.querySelector<HTMLInputElement>('input[aria-label="Search memory and code"]');
  check(draft && query, 'Both panel inputs must exist');
  return { draft, query };
}
function typeInto(element: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(prototype, 'value')!.set!;
  act(() => {
    setter.call(element, value);
    element.dispatchEvent(new Event('input', { bubbles: true }));
  });
}
function tick() {
  now += 1000;
  act(() => { for (const callback of [...intervals.values()]) callback(); });
}

let failure: unknown;
let unmounted = false;
try {
  render(null);
  panels(0);
  // Revisit all three empty projects, including same-project polling snapshots.
  for (let cycle = 0; cycle < 3; cycle++) {
    for (const project of ['project-a', 'project-b', 'project-c', 'project-a']) {
      const data = snapshot(project);
      render(data);
      panels(1);
      const initial = fields();
      check(initial.draft.value === '' && initial.query.value === '', 'Project state did not reset');
      typeInto(initial.draft, `unsaved ${project}`);
      typeInto(initial.query, `query ${project}`);
      for (let i = 1; i <= 30; i++) {
        tick();
        panels(1);
        render(i % 2 ? data : snapshot(project, i), i % 3 ? null : 'offline');
        panels(1);
        const current = fields();
        check(current.draft === initial.draft && current.query === initial.query,
          'Same-project rerender remounted a panel');
        check(current.draft.value === `unsaved ${project}` && current.query.value === `query ${project}`,
          'Same-project rerender lost local state');
      }
    }
    render(null);
    panels(0);
    tick();
    panels(0);
  }
  // Unmount with populated panels, then advance time to detect orphaned clocks.
  render(snapshot('project-a'));
  panels(1);
  act(() => root.unmount());
  unmounted = true;
  check(intervals.size === 0, 'Activity clock survived unmount');
  check(listeners.length === 0, 'Global listener survived unmount');
  tick();
  check(container.childNodes.length === 0, 'DOM survived unmount');
  check(errors.length === 0, `React errors: ${errors.join('\n')}`);
} catch (error) {
  failure = error;
} finally {
  if (!unmounted) act(() => root.unmount());
  window.setInterval = original.setInterval;
  window.clearInterval = original.clearInterval;
  Date.now = original.now;
  console.error = original.error;
  EventTarget.prototype.addEventListener = original.add;
  EventTarget.prototype.removeEventListener = original.remove;
}
const result = document.createElement('pre');
result.id = 'reconciliation-result';
result.textContent = failure ? `FAIL: ${String(failure)}\n${errors.join('\n')}` : 'PASS';
document.body.append(result);
