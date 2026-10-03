import { afterEach, describe, expect, it, vi } from 'vitest';
import * as fs from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import stream from './tunneldock-stream';
import flow from './tunneldock-flow';
vi.mock('node:fs',async(importOriginal)=>{
  const actual=await importOriginal<typeof import('node:fs')>();
  return {...actual,renameSync:vi.fn(actual.renameSync)};
});

const A = '11111111-1111-4111-8111-111111111111';
const B = 'wfr_01a101482dd47191a9c966d720d97f19';
const roots: string[] = [];
afterEach(() => { vi.mocked(fs.renameSync).mockReset(); vi.restoreAllMocks(); vi.useRealTimers(); for (const root of roots.splice(0)) fs.rmSync(root,{recursive:true,force:true}); });
function fixture(policy: Record<string, unknown> = {enabled:true,max_calls:2}) {
  const cwd = fs.mkdtempSync(join(tmpdir(),'td-stream-test-')); roots.push(cwd);
  const dir = join(cwd,'.tunneldock'); fs.mkdirSync(dir);
  const write = (name:string, value:unknown) => fs.writeFileSync(join(dir,name),JSON.stringify(value));
  write('session_binding.json',{project_id:'test-project',cwd});
  write('stream-policy.json',policy);
  write('web_status.json',{project_id:'test-project',session_binding:{cwd},state_token:'now',tasks:[],actions:[],transports:[]});
  const entries = new Map<string,any>(); let leaf: string | null = null;
  const ctx:any = {cwd,hasUI:true,ui:{notify:vi.fn(),setStatus:vi.fn()},sessionManager:{getCwd:()=>cwd,getSessionId:()=> 'test-session',getLeafId:()=>leaf,getEntry:(id:string)=>entries.get(id)}};
  const hooks: Record<string,Function> = {}; let tool:any;
  const install = () => stream({on:(name:string,fn:Function)=>{hooks[name]=fn;},registerTool:(t:any)=>{tool=t;}} as any);
  install();
  const entry = (call:string, request:string|undefined=A+'/one') => {
    const id=String(entries.size); entries.set(id,{type:'message',id,parentId:leaf,message:{role:'assistant',content:[{type:'toolCall',id:call,name:'read',arguments:{}}],chappie:request?{requestId:request}:undefined}}); leaf=id;
  };
  const call = async (id:string,name='read') => hooks.tool_call({type:'tool_call',toolCallId:id,toolName:name,input:{}},ctx);
  const result = async (id:string,text='ok',name='read',isError=false) => hooks.tool_result({type:'tool_result',toolCallId:id,toolName:name,input:{command:'SECRET'},content:[{type:'text',text}],details:{secret:'SECRET'},isError},ctx);
  const checkpoint = (request=A) => JSON.parse(fs.readFileSync(join(dir,'web_turns',request+'.json'),'utf8'));
  const recover = async (id:string,request_id?:string) => JSON.parse((await tool.execute(id,request_id?{request_id}:{},undefined,undefined,ctx)).content[0].text);
  return {cwd,dir,ctx,hooks,entry,call,result,checkpoint,recover,write,install};
}
describe('native stream guard',()=>{
  it('matches the current call before extracting identity; isolates interleaved requests',async()=>{
    const f=fixture(); f.entry('a'); f.entry('b',B+'/one');
    await f.call('a'); await f.result('a'); await f.call('b'); await f.result('b');
    expect(f.checkpoint().calls).toBe(1); expect(f.checkpoint(B).calls).toBe(1);
    expect(f.checkpoint().last_call_id).toBe('a');
  });
  it('explicitly leaves missing identities untracked and bounds ancestor traversal',async()=>{
    const f=fixture(); f.entry('known'); f.entry('missing','');
    expect(await f.call('missing')).toMatchObject({block:true,reason:expect.stringContaining('untracked')});
    for(let i=0;i<64;i++) f.entry('other-'+i,B+'/x');
    expect(await f.call('known')).toMatchObject({block:true,reason:expect.stringContaining('untracked')});
    expect(fs.existsSync(join(f.dir,'web_turns',A+'.json'))).toBe(false);
  });
  it('reserves exact call boundary, retains budget on reload/retry, permits only recovery',async()=>{
    const f=fixture(); f.entry('a'); expect(await f.call('a')).toBeUndefined(); await f.result('a');
    f.entry('b'); expect(await f.call('b')).toBeUndefined(); await f.result('b'); f.install();
    f.entry('c',A+'/retry'); expect(await f.call('c')).toMatchObject({block:true});
    expect(await f.call('a')).toMatchObject({block:true});
    f.entry('r'); expect(await f.call('r','tunneldock_recover')).toBeUndefined();
    await f.recover('r'); await f.result('r','recovery','tunneldock_recover');
    expect(f.checkpoint().calls).toBe(2); expect(f.checkpoint().last_call_name).toBe('read');
  });
  it('counts observed UTF-8 content and metadata; retains complete errors and ignores duplicate results',async()=>{
    const f=fixture({enabled:true,max_output_bytes:4}); f.entry('a'); await f.call('a');
    await f.result('a','秘密','read',true); const before=f.checkpoint(); await f.result('a','秘密','read',true);
    expect(before.output_bytes).toBe(Buffer.byteLength(JSON.stringify({content:[{type:'text',text:'\u79d8\u5bc6'}],details:{secret:'SECRET'},isError:true}))); expect(f.checkpoint().output_bytes).toBe(before.output_bytes); expect(before.last_error).toBe(true);
    expect(JSON.stringify(before)).not.toMatch(/SECRET|command/); expect(before.last_error_text).toBe('\u79d8\u5bc6');
    expect(Buffer.byteLength(JSON.stringify(before))).toBeLessThanOrEqual(8192);
    f.entry('b'); expect(await f.call('b')).toMatchObject({block:true});
  });
  it('blocks at elapsed boundary and validates policy without affecting opted-out projects',async()=>{
    vi.useFakeTimers(); const f=fixture({enabled:true,max_elapsed_ms:10}); f.entry('a'); await f.call('a'); await f.result('a');
    vi.advanceTimersByTime(10); f.entry('b'); expect(await f.call('b')).toMatchObject({block:true});
    f.write('stream-policy.json',{enabled:true,max_calls:1e100}); expect(await f.call('b')).toMatchObject({block:true,reason:expect.stringContaining('max_calls')});
    f.write('stream-policy.json',{enabled:false}); expect(await f.call('b')).toBeUndefined();
  });
  it('reads current authoritative receipts and active runs, never older project_room runs',async()=>{
    const f=fixture(); f.entry('r');
    f.write('project_room.json',{runs:[{id:'old-run',status:'completed',output_path:'old-handoff'}]});
    f.write('web_status.json',{project_id:'test-project',session_binding:{cwd:f.cwd},state_token:'current',tasks:[{id:'task',status:'queued'}],actions:[{kind:'review_task',task_id:'other',handoff_path:'current-handoff'}],transports:[{agent_id:'codex',status:'running',active_run_id:'current-run'}]});
    const r=await f.recover('r'); expect(JSON.stringify(r)).toContain('current-run'); expect(JSON.stringify(r)).toContain('current-handoff'); expect(JSON.stringify(r)).not.toContain('old-');
    expect(r.tasks[0].status).toBe('queued'); expect(r.tasks[0].run_id).toBeUndefined();
  });
  it('rejects cwd mismatch and symlinked output directories',async()=>{
    const f=fixture(); f.entry('a'); f.write('session_binding.json',{project_id:'test-project',cwd:join(f.cwd,'different')});
    expect(await f.call('a')).toMatchObject({block:true,reason:expect.stringContaining('cwd')}); await expect(f.recover('a')).rejects.toThrow(/cwd/);
    f.write('session_binding.json',{project_id:'test-project',cwd:f.cwd});
    const target=fs.mkdtempSync(join(tmpdir(),'td-stream-link-')); roots.push(target);
    fs.symlinkSync(target,join(f.dir,'web_turns'),process.platform==='win32'?'junction':'dir');
    expect(await f.call('a')).toMatchObject({block:true,reason:expect.stringContaining('Symlink')}); expect(fs.readdirSync(target)).toEqual([]);
  });
  it('reports persistence failure and blocks further execution without losing the prior checkpoint',async()=>{
    const f=fixture(); f.entry('a'); await f.call('a');
    vi.mocked(fs.renameSync).mockImplementation(()=>{throw Object.assign(new Error('disk failure'),{code:'EIO'});});
    const r=await f.result('a'); expect(JSON.stringify(r)).toContain('EIO'); expect(f.ctx.ui.notify).toHaveBeenCalled();
    vi.mocked(fs.renameSync).mockReset(); f.entry('b'); expect(await f.call('b')).toMatchObject({block:true,reason:expect.stringContaining('EIO')});
    expect(f.checkpoint().calls).toBe(1);
  });
  it('requires explicit selection of a prior checkpoint on a new browser turn without writing or replaying',async()=>{
    const f=fixture();f.entry('a');await f.call('a');await f.result('a');
    const before=fs.readFileSync(join(f.dir,'web_turns',A+'.json'),'utf8');
    f.entry('r',B+'/new');const r=await f.recover('r');
    expect(r.request_id).toBe(B);expect(r.current_request_id).toBe(B);expect(r.checkpoint_exists).toBe(false); expect(r.counters).toBeNull();
    expect(fs.readFileSync(join(f.dir,'web_turns',A+'.json'),'utf8')).toBe(before);
    expect(fs.existsSync(join(f.dir,'web_turns',B+'.json'))).toBe(false);
    expect((await f.recover('r',A)).request_id).toBe(A);
    await expect(f.recover('r','not-a-valid-id')).rejects.toThrow(/Invalid/);
  });
  it('bounds complete recovery JSON rather than only individual strings',async()=>{
    const f=fixture();f.entry('r');const huge='x'.repeat(240);
    f.write('web_status.json',{project_id:'test-project',session_binding:{cwd:f.cwd},state_token:'current',tasks:Array.from({length:8},()=>({id:huge,owner:huge,status:huge,review_mode:huge,reviewed_by:huge,updated_at:huge})),actions:Array.from({length:8},()=>({kind:huge,task_id:huge,consultation_id:huge,handoff_path:huge,finalizer_path:huge})),transports:[]});
    const result=await f.recover('r');expect(Buffer.byteLength(JSON.stringify(result))).toBeLessThanOrEqual(8192);expect(result.omitted_items).toBeGreaterThan(0);
  });
  it('reports item-cap omissions even when nine short tasks fit the byte budget',async()=>{
    const f=fixture();f.entry('r');
    f.write('web_status.json',{project_id:'test-project',session_binding:{cwd:f.cwd},state_token:'current',tasks:Array.from({length:9},(_,i)=>({id:'task-'+i,status:'active'})),actions:[],transports:[]});
    const r=await f.recover('r');expect(r.tasks).toHaveLength(8);expect(r.omitted_items).toBe(1);
  });
  it('latches admission checkpoint failures so later calls cannot silently resume execution',async()=>{
    const f=fixture();f.entry('a');
    vi.mocked(fs.renameSync).mockImplementation(()=>{throw Object.assign(new Error('disk failure'),{code:'EIO'});});
    expect(await f.call('a')).toMatchObject({block:true,reason:expect.stringContaining('EIO')});
    vi.mocked(fs.renameSync).mockReset();f.entry('b');
    expect(await f.call('b')).toMatchObject({block:true,reason:expect.stringContaining('EIO')});
    f.entry('r');expect((await f.recover('r')).error).toContain('EIO');
  });
  it('exposes unresolved reservations without claiming failure or replaying them',async()=>{
    const f=fixture();f.entry('a');await f.call('a');f.entry('r');
    const r=await f.recover('r');expect(r.pending_count).toBe(1);expect(r.finished_count).toBe(0);
    expect(r.execution_state).toBe('EXECUTION_UNCERTAIN');expect(r.next_action).toContain('never assume safe replay');expect(r.next_action).toContain('Browser transport and upstream 429 are not repaired');
  });
  it('records a loaded-file capability without opting the project in',async()=>{
    const f=fixture({enabled:false}); await f.hooks.session_start({},f.ctx);
    const cap=JSON.parse(fs.readFileSync(join(f.dir,'stream-capability.json'),'utf8'));
    expect(cap).toMatchObject({version:1,pid:process.pid,session_id:'test-session',enabled:false}); expect(cap.fingerprint).toMatch(/^[a-f0-9]{64}$/);
  });
  it('retains exact batched admission identity beyond 64 newer entries',async()=>{
    const f=fixture(); f.entry('a'); f.entry('b'); await f.call('a'); await f.call('b');
    for(let i=0;i<70;i++) f.entry('new-'+i,B+'/other-client');
    await f.result('b'); await f.result('a');
    expect(f.checkpoint()).toMatchObject({calls:2,pending:[],last_call_id:'a'});
    expect(f.checkpoint().finished).toHaveLength(2);
    expect(fs.existsSync(join(f.dir,'web_turns',B+'.json'))).toBe(false);
  });
  it('reports reservation uncertainty, observed results and original session separately without recovery writes',async()=>{
    const f=fixture(); f.entry('a'); await f.call('a'); f.entry('r');
    const pending=await f.recover('r');
    expect(pending).toMatchObject({execution_state:'EXECUTION_UNCERTAIN',pending_count:1,finished_count:0,last_call_id:null});
    const error='Failure at C:\\evidence\\'+ 'long-directory-'.repeat(30)+'\\HANDOFF.md';
    await f.result('a',error,'read',true);
    f.ctx.sessionManager.getSessionId=()=> 'new-session'; f.entry('r2',B+'/new');
    const before=fs.readFileSync(join(f.dir,'web_turns',A+'.json'),'utf8');
    const r=await f.recover('r2',A);
    expect(r).toMatchObject({session_id:'new-session',checkpoint_session_id:'test-session',execution_state:'RESULT_OBSERVED',pending_count:0,finished_count:1,last_call_id:'a',last_call_name:'read',last_error:true,last_error_text:error,last_error_text_omitted:false});
    expect(r.next_action).toContain('not scientific acceptance');
    expect(fs.readFileSync(join(f.dir,'web_turns',A+'.json'),'utf8')).toBe(before);
  });
  it('latches unresolved result accounting durably and recovery never clears the latch',async()=>{
    const f=fixture(); f.entry('lost','');
    expect(JSON.stringify(await f.result('lost'))).toContain('EXECUTION_UNCERTAIN');
    const path=join(f.dir,'stream-failure-test-session.json'); const before=fs.readFileSync(path,'utf8');
    f.install(); f.entry('b',B+'/new');
    expect(await f.call('b')).toMatchObject({block:true,reason:expect.stringContaining('Persisted admission failure latch')});
    expect(await f.recover('b')).toMatchObject({execution_state:'EXECUTION_UNCERTAIN',error:expect.stringContaining('Unresolved tool result identity')});
    expect(fs.readFileSync(path,'utf8')).toBe(before);
  });
  it('latches a result with known identity but no reservation instead of silently skipping accounting',async()=>{
    const f=fixture(); f.entry('unreserved');
    expect(JSON.stringify(await f.result('unreserved'))).toContain('no exact in-process admission');
    f.install(); f.entry('next'); expect(await f.call('next')).toMatchObject({block:true});
    expect(fs.existsSync(join(f.dir,'web_turns',A+'.json'))).toBe(false);
  });
  it('accounts admitted results after policy disable and never reassigns a pending call ID',async()=>{
    const f=fixture(); f.entry('a'); await f.call('a'); f.entry('a',B+'/other-client');
    expect(await f.call('a')).toMatchObject({block:true,reason:expect.stringContaining('already reserved')});
    f.write('stream-policy.json',{enabled:false}); await f.result('a');
    expect(f.checkpoint()).toMatchObject({calls:1,pending:[],last_call_id:'a'});
    expect(fs.existsSync(join(f.dir,'web_turns',B+'.json'))).toBe(false);
  });
  it('persists a result-save failure across reload when latch publication succeeds',async()=>{
    const f=fixture(); f.entry('a'); await f.call('a');
    vi.mocked(fs.renameSync).mockImplementationOnce(()=>{throw new Error('full failure path C:\\evidence\\artifact.json');});
    await f.result('a'); f.install(); f.entry('b',B+'/new');
    expect(await f.call('b')).toMatchObject({block:true,reason:expect.stringContaining('C:\\evidence\\artifact.json')});
    expect((await f.recover('b',A)).pending_count).toBe(1);
  });
  it('states durability limitation when disk cannot publish the latch and retains pending uncertainty on reload',async()=>{
    const f=fixture(); f.entry('a'); await f.call('a');
    vi.mocked(fs.renameSync).mockImplementation(()=>{throw Object.assign(new Error('disk unavailable'),{code:'EIO'});});
    expect(JSON.stringify(await f.result('a'))).toContain('cannot be guaranteed after reload');
    vi.mocked(fs.renameSync).mockReset(); f.install(); f.entry('b');
    expect(await f.call('b')).toMatchObject({block:true,reason:expect.stringContaining('EXECUTION_UNCERTAIN')});
    expect(f.checkpoint().pending).toHaveLength(1);
  });
  it('preserves complete artifact paths and explicitly omits over-budget error text',async()=>{
    const f=fixture(); const path='C:\\evidence\\'+'directory-'.repeat(40)+'\\HANDOFF.md';
    f.write('web_status.json',{project_id:'test-project',session_binding:{cwd:f.cwd},tasks:[],transports:[],actions:[{handoff_path:path}]});
    f.entry('a'); await f.call('a'); await f.result('a','error'.repeat(3000),'read',true);
    f.entry('r'); const r=await f.recover('r');
    expect(r.actions[0].handoff_path).toBe(path); expect(r.last_error_text).toBeNull(); expect(r.last_error_text_omitted).toBe(true);
    f.write('web_status.json',{project_id:'test-project',session_binding:{cwd:f.cwd},tasks:[],transports:[],actions:[{handoff_path:path.repeat(30)}]});
    const omitted=await f.recover('r'); expect(omitted.actions).toEqual([]); expect(omitted.omitted_items).toBe(1);
  });
  it.each([true,false])('preserves images and accounts content/details at its hook boundary (flow first=%s)',async(flowFirst)=>{
    const f=fixture({enabled:true,max_output_bytes:100}); f.entry('a'); await f.call('a');
    let flowHook:Function=()=>undefined;
    flow({on:(_name:string,fn:Function)=>{flowHook=fn;}} as any);
    const image={type:'image',data:'YWJj'.repeat(100),mimeType:'image/png'};
    let event:any={toolCallId:'a',toolName:'read',content:[{type:'text',text:'large evidence '.repeat(1000)},image],details:{evidence:'details'.repeat(2000)},isError:false};
    let observed=0;
    for(const hook of (flowFirst?[flowHook,f.hooks.tool_result]:[f.hooks.tool_result,flowHook])) {
      if(hook===f.hooks.tool_result) observed=Buffer.byteLength(JSON.stringify({content:event.content,details:event.details,isError:event.isError}));
      event={...event,...await hook(event,f.ctx)};
    }
    expect(event.content).toContainEqual(image); expect(event.details.truncated).toBe(true);
    expect(fs.existsSync(event.details.fullDetailsPath)).toBe(true);
    expect(f.checkpoint().output_bytes).toBe(observed);
    f.entry('b'); expect(await f.call('b')).toMatchObject({block:true});
  });
});
