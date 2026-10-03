import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { I18nProvider } from '../../i18n';
import { CampaignPanel } from './CampaignPanel';
import type { CampaignSummary } from '../../types/activity';

const campaign: CampaignSummary = {
  id: 'fixture', plan_sha256: 'a'.repeat(64), question: 'Frozen IQA comparison', stage: 'baseline', state: 'active',
  paused: false, active_drain: false, attempts: 1, budget: 3, deadline: '2030-01-01T00:00:00Z',
  execution: 'execution_success', evidence: 'evidence_verified', gate: true, metric: 0.8,
  evidence_ids: ['b'.repeat(64)], web_reviewed: false, accepted: false, reason: '', authorization_pending: false,
  next_owner: 'chatgpt', next_action: 'Review current evidence',
};
const render = (patch: Partial<CampaignSummary> = {}) => renderToStaticMarkup(<I18nProvider initialLocale="en-US"><CampaignPanel projectId="fixture" status={{ items: [{ ...campaign, ...patch }] }}/></I18nProvider>);

describe('CampaignPanel evidence and authorization boundaries', () => {
  it('remounts campaign state when the project changes', () => {
    const first = CampaignPanel({ projectId: 'first', status: { items: [campaign] } });
    const second = CampaignPanel({ projectId: 'second', status: { items: [campaign] } });
    expect(first.key).toBe('first'); expect(second.key).toBe('second');
    expect(first.key).not.toBe(second.key);
  });
  it('uses the existing Chinese locale for authorization, pause and review states', () => {
    const html = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><CampaignPanel projectId="fixture" status={{ items: [{ ...campaign, paused: true, room_enabled: false, next_owner: 'user', next_action: 'resume' }] }}/></I18nProvider>);
    expect(html).toContain('研究计划'); expect(html).toContain('执行成功'); expect(html).toContain('证据已校验');
    expect(html).toContain('待审核／未接受'); expect(html).toContain('恢复调度'); expect(html).toContain('房间已禁用');
    const draft = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><CampaignPanel projectId="fixture" status={{ items: [{ ...campaign, state: 'draft', authorization_pending: true }] }}/></I18nProvider>);
    expect(draft).toContain('查看草案并准备授权'); expect(draft).not.toContain('授权此精确计划并启用有界调度');
  });
  it('distinguishes loading from empty state and supports compact evidence counts', () => {
    const loading = renderToStaticMarkup(<I18nProvider initialLocale="en-US"><CampaignPanel projectId="fixture"/></I18nProvider>);
    expect(loading).toContain('Waiting for campaign status.'); expect(loading).not.toContain('No recorded campaigns.');
    const empty = renderToStaticMarkup(<I18nProvider initialLocale="en-US"><CampaignPanel projectId="fixture" status={{ items: [] }}/></I18nProvider>);
    expect(empty).toContain('No recorded campaigns.');
    const compact = render({ evidence_ids: undefined, evidence_count: 32 });
    expect(compact).toContain('Evidence verified');
  });
  it('keeps successful execution and a passing gate separate from Web acceptance', () => {
    const html = render();
    expect(html).toContain('Execution succeeded'); expect(html).toContain('Evidence verified');
    expect(html).toContain('PASS'); expect(html).toContain('pending / not accepted');
    expect(html).not.toContain('100%');
  });
  it('requires draft inspection before showing the exact-plan authorization action', () => {
    const html = render({ state: 'draft', authorization_pending: true, attempts: 0, execution: null, evidence: null, gate: null, metric: null });
    expect(html).toContain('Review draft for authorization'); expect(html).toContain('unknown');
    expect(html).not.toContain('Authorize this exact plan and enable bounded scheduling');
    expect(html).not.toContain('Pause scheduling');
  });
  it('reports scientific failure, its reason and active drain without implying process termination', () => {
    const html = render({ gate: false, paused: true, active_drain: true, reason: 'Valid negative result' });
    expect(html).toContain('FAIL'); expect(html).toContain('Valid negative result');
    expect(html).toContain('does not terminate processes'); expect(html).toContain('Resume scheduling');
  });
  it('exposes disk errors instead of showing a fabricated healthy campaign', () => {
    const html = renderToStaticMarkup(<I18nProvider initialLocale="en-US"><CampaignPanel projectId="fixture" status={{ items: [], error: 'Corrupt campaign identity/state' }}/></I18nProvider>);
    expect(html).toContain('role="alert"'); expect(html).toContain('Corrupt campaign identity/state');
  });
  it('shows an actual negative Web review without relabeling the gate', () => {
    const html = render({ gate: false, web_reviewed: true, accepted: false, state: 'reviewed' });
    expect(html).toContain('reviewed / not accepted'); expect(html).toContain('FAIL');
    expect(html).not.toContain('Pause scheduling');
  });
});
