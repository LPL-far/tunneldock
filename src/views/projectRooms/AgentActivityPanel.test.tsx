import { renderToStaticMarkup } from 'react-dom/server';
import { describe, it, expect } from 'vitest';
import { AgentActivityPanel } from './AgentActivityPanel';
import { I18nProvider } from '../../i18n';
import type { ProjectActivity, TaskActivity } from '../../types/activity';
const task = {task_id:'t',run_id:'r',agent_id:'codex',title:'Projection repair',phase:'received',task_status:'review',kind:'work',attempts:2,handoff_available:true,
 received_at:'2026-10-03T12:00:00Z',started_at:'2026-10-03T11:00:00Z',progress:null,detail:'',error:null,web_reviewed:false,reviewed_by:''} as TaskActivity;
const data={project_id:'fixture',revision:'rev',sampled_at:'2026-10-03T12:01:00Z',pending_review_count:1,pending_reviews:[task],cards:[
 {agent_id:'codex',current:task,queued:0,human_thread_id:'human-id',automation_thread_id:'auto-id',transport:'healthy'},
 {agent_id:'gemini',current:{...task,agent_id:'gemini',title:'Independent validation',phase:'blocked',detail:'Validation failed'},queued:0,human_thread_id:null,automation_thread_id:'cascade-id',transport:'healthy'}]} as ProjectActivity;
describe('live activity panel rendering',()=>{
 it('shows both agents, actual receipt, pending review and thread identity',()=>{
 const html=renderToStaticMarkup(<I18nProvider><AgentActivityPanel data={data} error={null} cwd="D:/fixture"/></I18nProvider>);
 for(const text of ['Codex','Antigravity / Gemini','Projection repair','Independent validation','human-id','auto-id','cascade-id','Validation failed'])expect(html).toContain(text);
 expect(html).not.toContain('100%');
 });
 it('keeps the last known activity visible on a refresh failure',()=>{
 const html=renderToStaticMarkup(<I18nProvider><AgentActivityPanel data={data} error="offline" cwd="D:/fixture"/></I18nProvider>);
 expect(html).toContain('role="alert"');expect(html).toContain('Projection repair');
 });
});
