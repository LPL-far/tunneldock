import { it, expect } from 'vitest';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { receipts, receiptState, waitForReceipts } from './tunneldock-wait';
function fixture() {
 const root=mkdtempSync(join(tmpdir(),'td-wait-'));mkdirSync(join(root,'.tunneldock'));
 writeFileSync(join(root,'.tunneldock/session_binding.json'),JSON.stringify({cwd:root,project_id:'fixture'}));
 return root;
}
it('does not return an older successful handoff for a queued revision',()=>{
 const s={tasks:[{id:'TASK-1',status:'queued'}],runs:[{id:'R',task_id:'TASK-1',status:'completed',started_at:'2026-10-03T01:00Z',output_path:'unused-handoff.md'}]};
 const r=receipts(s,['TASK-1']);expect(r[0].available).toBe(false);expect(r[0].run_id).toBeNull();expect(receiptState(r)).toBe('waiting');
});
it('waits for all requested workers; blocked is not consensus',()=>{
 const items=[{task_id:'TASK-1',status:'review',available:true,run_id:'r',handoff:'p',agent:'codex'}, {task_id:'TASK-2',status:'active',available:false,run_id:'s',handoff:null,agent:'gemini'}];
 expect(receiptState(items)).toBe('waiting');items[1].status='blocked';expect(receiptState(items)).toBe('blocked');
});
it('returns complete receipt paths without changing task/review state',async()=>{
 const root=fixture();try{
 const output=join(root,'HANDOFF.md');writeFileSync(output,'finite losses and verification evidence');
 const snapshot={tasks:[{id:'TASK-1',status:'review',owner:'codex'}],runs:[{id:'RUN-1',task_id:'TASK-1',status:'completed',started_at:'2026-10-03T01:00Z',output_path:output}]};
 writeFileSync(join(root,'.tunneldock/project_room.json'),JSON.stringify(snapshot));
 const result=await waitForReceipts(root,{task_ids:['TASK-1'],max_wait_seconds:0});
 expect(result.state).toBe('ready_for_review');expect(result.receipts[0].handoff).toBe(output);expect(result.next_action).toContain('THIS Web turn');
 }finally{rmSync(root,{recursive:true,force:true});}
});
it('timeout and cancellation do not pretend the worker completed',async()=>{
 const root=fixture();try{
 writeFileSync(join(root,'.tunneldock/project_room.json'),JSON.stringify({tasks:[{id:'TASK-1',status:'active'}],runs:[]}));
 expect((await waitForReceipts(root,{task_ids:['TASK-1'],max_wait_seconds:0})).state).toBe('timeout');
 const controller=new AbortController();controller.abort();
 expect((await waitForReceipts(root,{task_ids:['TASK-1']},controller.signal)).state).toBe('cancelled');
 }finally{rmSync(root,{recursive:true,force:true});}
});
it('refuses cross-project identity and missing task IDs',async()=>{
 const root=fixture();try{
 await expect(waitForReceipts(root,{task_ids:[]})).rejects.toThrow();
 writeFileSync(join(root,'.tunneldock/session_binding.json'),JSON.stringify({cwd:join(root,'other'),project_id:'other'}));
 await expect(waitForReceipts(root,{task_ids:['TASK-1']})).rejects.toThrow('binding');
 }finally{rmSync(root,{recursive:true,force:true});}
});
