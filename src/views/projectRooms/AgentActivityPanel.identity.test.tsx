import { isValidElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { I18nProvider } from '../../i18n';
import type { ProjectActivity } from '../../types/activity';
import { AgentActivityPanel } from './AgentActivityPanel';
import { CampaignPanel } from './CampaignPanel';
import { ContextEnginePanel } from './ContextEnginePanel';

function snapshot(projectId: string, revision: number): ProjectActivity {
  return {
    version: 1, project_id: projectId, revision: String(revision),
    sampled_at: new Date(1_790_000_000_000 + revision * 1000).toISOString(),
    cards: [], pending_reviews: [], pending_review_count: 0,
    campaigns: { items: [], total: 0, revision },
  };
}

// SSR omits keys from HTML. Capture the actual sibling elements inside a React
// render so this regression also runs with the project's default Node runner.
function panelKeys(data: ProjectActivity) {
  let keys: (string | null)[] = [];
  function Capture() {
    const element = AgentActivityPanel({ data, error: null, cwd: 'D:/fixture' });
    const children = element.props.children as ReactNode[];
    keys = children.filter(isValidElement)
      .filter(child => child.type === CampaignPanel || child.type === ContextEnginePanel)
      .map(child => child.key);
    return element;
  }
  renderToStaticMarkup(<I18nProvider initialLocale="en-US"><Capture/></I18nProvider>);
  return keys;
}

describe('activity panel sibling identity', () => {
  it('keeps component-specific keys unique and stable across refreshes and project switches', () => {
    const first = panelKeys(snapshot('first', 0));
    expect(first).toHaveLength(2);
    expect(first.every(key => key !== null)).toBe(true);
    expect(new Set(first).size).toBe(2);
    for (let revision = 1; revision <= 10; revision++) {
      expect(panelKeys(snapshot('first', revision))).toEqual(first);
      const second = panelKeys(snapshot('second', revision));
      expect(new Set([...first, ...second]).size).toBe(4);
      expect(second).toEqual(panelKeys(snapshot('second', revision + 1)));
      expect(panelKeys(snapshot('first', revision + 1))).toEqual(first);
    }
  });

});
