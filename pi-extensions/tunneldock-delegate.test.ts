import {expect,it,vi} from 'vitest';
import delegate,{delegationEvent,shouldDelegate} from './tunneldock-delegate';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
const input={action:'create' as const,request_id:'repair-job-1',title:'Persistent operation',goal:'Implement and verify the bounded job.',write_scope:['tools/job.py']};
it('creates real scoped auto-dispatch with conservative Web review',()=>{const e=delegationEvent(input);expect(e.auto_dispatch).toBe(true);expect(e.review_mode).toBe('web');expect(e.thread_id).toBe('DELEGATE-repair-job-1');expect(delegationEvent(input)).toEqual(e);});
it('rejects unbounded, escaping and invalid delegation',()=>{for(const changes of [{request_id:'../bad'},{goal:''},{write_scope:[]},{write_scope:['../other']},{write_scope:['C:/other']}])expect(()=>delegationEvent({...input,...changes})).toThrow();});
it('routes edits to workers but keeps reviewer memory and inbox writes',()=>{expect(shouldDelegate('edit',{path:'src/model.py'},'/work/project')).toBe(true);expect(shouldDelegate('write',{path:'.project_memory/PROJECT_STATE.md'},'/work/project')).toBe(false);expect(shouldDelegate('write',{path:'.tunneldock/inbox/a.json'},'/work/project')).toBe(false);expect(shouldDelegate('edit',{path:'../another.py'},'/work/project')).toBe(true);expect(shouldDelegate('read',{path:'src/model.py'},'/work/project')).toBe(false);});
it('routes known remote execution rather than ordinary evidence inspection',()=>{expect(shouldDelegate('bash',{command:"ssh host 'python train.py'"},'/work')).toBe(true);expect(shouldDelegate('bash',{command:'scp x host:x'},'/work')).toBe(true);expect(shouldDelegate('bash',{command:"ssh host 'tail -n 20 log'"},'/work')).toBe(false);});
it.each(['queued','active','blocked','superseded','backlog','failed','review','completed'])('exposes completed handoff only for current eligible task state %s',async(status)=>{
 const cwd=mkdtempSync(join(tmpdir(),'td-delegate-test-'));
 try {
  vi.stubEnv('APPDATA',cwd);
  const dir=join(cwd,'.tunneldock'),store=join(cwd,'TunnelDock/projects/test-project');
  mkdirSync(dir);mkdirSync(store,{recursive:true});
  const path=join(cwd,'HANDOFF.md');writeFileSync(path,'Evidence awaiting review');
  writeFileSync(join(dir,'session_binding.json'),JSON.stringify({project_id:'test-project',cwd}));
  writeFileSync(join(store,'tasks.json'),JSON.stringify([{id:'task',thread_id:'DELEGATE-test',status,summary:'blocked at '+path}]));
  const runs=[{id:'run',task_id:'task',started_at:'2026-01-01',status:'completed',output_path:path}];
  writeFileSync(join(store,'runs.json'),JSON.stringify(runs));
  let tool:any; delegate({on:()=>{},registerTool:(t:any)=>{tool=t;}} as any);
  const read=async()=>JSON.parse((await tool.execute('call',{action:'status',request_id:'test'},undefined,undefined,{cwd})).content[0].text);
  expect((await read()).handoff).toBe(['review','completed'].includes(status)?path:null);
  runs.push({id:'new-run',task_id:'task',started_at:'2026-02-01',status:'running',output_path:path});
  writeFileSync(join(store,'runs.json'),JSON.stringify(runs));
  expect((await read()).handoff).toBeNull();
 } finally {vi.unstubAllEnvs();rmSync(cwd,{recursive:true,force:true});}
});
