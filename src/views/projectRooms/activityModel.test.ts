import { describe, it, expect } from 'vitest';
import { duration, phaseStep, reviewPrompt, stateSignature } from './activityModel';
import type { ProjectActivity, TaskActivity } from '../../types/activity';
const item = {task_id:'t', run_id:'r2', handoff_available:true, agent_id:'codex', phase:'received', task_status:'review'} as TaskActivity;
describe('agent activity', () => {
  it('never marks returned work as Web-reviewed', () => { expect(phaseStep('received')).toBe(2); expect(phaseStep('reviewed')).toBe(3); expect(phaseStep('blocked')).toBe(-1); });
  it('includes exact attempt and prohibits premature acceptance', () => {const p=reviewPrompt('D:/3d',[item],'zh-CN'); expect(p).toContain('run_id=r2'); expect(p).toContain('review.started'); expect(p).toContain('审核完成后');});
  it('does not fabricate ETA or a negative duration', () => {expect(duration(null,0)).toBe('—'); expect(duration('bad',0)).toBe('—'); expect(duration('2026-01-01T00:00:00Z',Date.parse('2026-01-01T00:02:05Z'))).toBe('2m 5s');});
  it('only lifecycle changes reload a full room', () => {const a={cards:[{current:item,queued:0}]} as ProjectActivity;
    const b={cards:[{current:{...item,progress:{units:2,event:'tool_started',last_event_at:'now'}},queued:0}]} as ProjectActivity;
    expect(stateSignature(a)).toBe(stateSignature(b)); b.cards[0].current!.run_id='r3'; expect(stateSignature(a)).not.toBe(stateSignature(b));
  });
});
