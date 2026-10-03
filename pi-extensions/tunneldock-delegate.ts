// Project-local execution routing. Not a security sandbox and not a scientific reviewer.
import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { readFileSync, writeFileSync, mkdirSync, existsSync, renameSync, lstatSync } from 'node:fs';
import { join, resolve, relative, isAbsolute } from 'node:path';
import { homedir } from 'node:os';
import { createHash } from 'node:crypto';
import { receipts } from './tunneldock-wait';
interface Input { action:'create'|'status'; request_id:string; title?:string; goal?:string; owner?:'codex'|'gemini'; write_scope?:string[]; review_mode?:'web'|'auto' }
const idPattern=/^[a-zA-Z0-9_-]{1,80}$/;
function json(path:string){return JSON.parse(readFileSync(path,'utf8'));}
function identity(cwd:string) {
 const b=json(join(cwd,'.tunneldock/session_binding.json'));
 if(!idPattern.test(b.project_id||'') || resolve(b.cwd).toLowerCase()!==resolve(cwd).toLowerCase())throw Error('Project binding mismatch');
 return b;
}
export function delegationEvent(p:Input) {
 if(!idPattern.test(p.request_id))throw Error('request_id must be 1-80 letters, digits, hyphens or underscores');
 if(!p.title?.trim() || p.title.length>180 || !p.goal?.trim() || p.goal.length>10000)throw Error('Explicit title and bounded completion goal required');
 if(!p.write_scope?.length || p.write_scope.length>16 || p.write_scope.some(s=>!s.trim()||s.length>200||s.includes('..')||isAbsolute(s)||/^[a-zA-Z]:/.test(s)))throw Error('Provide bounded relative write scopes');
 if(!['codex','gemini'].includes(p.owner||'codex')||!['web','auto'].includes(p.review_mode||'web'))throw Error('Invalid owner/review mode');
 return {kind:'task.create',author:'chatgpt',title:p.title,goal:p.goal,owner:p.owner||'codex',write_scope:p.write_scope,reviewers:['chatgpt'],review_mode:p.review_mode||'web',auto_dispatch:true,request_id:p.request_id,thread_id:'DELEGATE-'+p.request_id};
}
export function shouldDelegate(name:string,input:any,cwd:string):boolean {
 if(name==='write'||name==='edit') {
  if(typeof input?.path!=='string')return false;
  const path=relative(resolve(cwd),resolve(cwd,input.path)).replace(/\\/g,'/');
  if(path.startsWith('../')||isAbsolute(path))return true;
  // Web retains explicit coordination and canonical-memory review responsibility.
  return !(path.startsWith('.project_memory/')||path.startsWith('.tunneldock/inbox/')||path.startsWith('.tunneldock/diagnostics/'));
 }
 // Deliberately limited heuristic: known remote execution/upload paths, not every shell program.
 return name==='bash' && typeof input?.command==='string' && (/\b(?:scp|rsync)\s/i.test(input.command)||(/\bssh\s/i.test(input.command)&&/\b(?:python\d?|torchrun|nohup|sbatch|kill|rm|tee)\b/i.test(input.command)));
}
function scopePolicy(cwd:string) {
 try {identity(cwd);return json(join(cwd,'.tunneldock/execution-policy.json')).delegate_execution===true;}catch{return false;}
}
export default function(pi:ExtensionAPI) {
 pi.on('tool_call',async(event:any,ctx:any)=>{
  if(scopePolicy(ctx.cwd)&&shouldDelegate(event.toolName,event.input,ctx.cwd))return {block:true,reason:'This Project Room delegates code edits, uploads and remote execution to a worker. Use tunneldock_delegate(action=create, stable request_id, explicit goal and write_scope). Web may read evidence, write coordination events and update reviewed canonical memory. Do not bypass with another shell. Explicit maintenance can disable delegate_execution in project policy.'};
 });
 pi.on('session_start',async(_event:any,ctx:any)=>{
  try {const b=identity(ctx.cwd);const root=join(ctx.cwd,'.tunneldock');mkdirSync(root,{recursive:true});writeFileSync(join(root,'delegation-capability.json'),JSON.stringify({project_id:b.project_id,pid:process.pid,loaded_at:new Date().toISOString(),tool:'tunneldock_delegate',version:1}));}catch(e){if(existsSync(join(ctx.cwd,'.tunneldock/session_binding.json')))console.error('[tunneldock_delegate] capability recording failed',String(e));}
 });
 pi.registerTool({name:'tunneldock_delegate',label:'Delegate real execution to an agent',description:'Create ONE persistent Codex/Gemini task with an explicit goal and scope, or inspect its receipt. Reuse request_id for safe retry; never turn an uncertain submission into a duplicate task. Uses the existing Project Room scheduler and exposes real task/run state. Does not accept results or wake an inactive browser.',
 parameters:{type:'object',required:['action','request_id'],additionalProperties:false,properties:{action:{type:'string',enum:['create','status']},request_id:{type:'string',pattern:'^[a-zA-Z0-9_-]{1,80}$'},title:{type:'string',maxLength:180},goal:{type:'string',maxLength:10000},owner:{type:'string',enum:['codex','gemini']},write_scope:{type:'array',minItems:1,maxItems:16,items:{type:'string'}},review_mode:{type:'string',enum:['web','auto']}}} as any,
 async execute(_id:string,p:Input,signal:AbortSignal|undefined,_update:unknown,ctx:{cwd:string}) {
  const b=identity(ctx.cwd);if(!idPattern.test(p.request_id))throw Error('Invalid request ID');
  const store=join(process.env.APPDATA||join(homedir(),'.local/share'),'TunnelDock/projects',b.project_id);
  const find=()=>{const tasks=json(join(store,'tasks.json'));return tasks.find((t:any)=>t.thread_id==='DELEGATE-'+p.request_id);};
  const requests=join(ctx.cwd,'.tunneldock/delegations');mkdirSync(requests,{recursive:true});if(lstatSync(requests).isSymbolicLink())throw Error('Symlinked delegation directory refused');
  const receiptPath=join(requests,p.request_id+'.json');let task=find();
  if(p.action==='create') {
   const event=delegationEvent(p);const fingerprint=createHash('sha256').update(JSON.stringify(event)).digest('hex');
   if(existsSync(receiptPath)){const prior=json(receiptPath);if(prior.fingerprint!==fingerprint)throw Error('request_id already used for different work');}
   else {writeFileSync(receiptPath,JSON.stringify({fingerprint,event,created_at:new Date().toISOString()}),{flag:'wx'});}
   if(!task){const inbox=join(ctx.cwd,'.tunneldock/inbox');mkdirSync(inbox,{recursive:true});if(lstatSync(inbox).isSymbolicLink())throw Error('Symlinked inbox refused');const path=join(inbox,'delegate-'+p.request_id+'.json');const tmp=path+'.'+process.pid+'.tmp';writeFileSync(tmp,JSON.stringify(event));renameSync(tmp,path);}
   const until=Date.now()+8000;
   while(!task && Date.now()<until && !signal?.aborted){await new Promise(r=>setTimeout(r,400));task=find();const errorPath=join(ctx.cwd,'.tunneldock/inbox_error.json');if(existsSync(errorPath)){const e=json(errorPath);if(String(e.file).includes('delegate-'+p.request_id+'.json'))throw Error(String(e.error));}}
  }
  const runs=json(join(store,'runs.json'));const run=task?runs.filter((r:any)=>r.task_id===task.id).sort((a:any,b:any)=>Date.parse(b.started_at)-Date.parse(a.started_at))[0]:null;
  const receipt=task?receipts({tasks:[task],runs},[task.id])[0]:null;
  const result={project_id:b.project_id,request_id:p.request_id,task_id:task?.id??null,status:task?.status??'submission_unconfirmed',run_id:receipt?.run_id??null,run_status:receipt?.run_id?run?.status??null:null,handoff:receipt?.handoff??null,error:task?.status==='blocked'?String(task.summary||run?.error_message||''):null,next_action:task?.status==='blocked'?'Resolve the recorded blocker before waiting; no running task is implied.':task?'Wait for this task, then review its current run evidence.':'Keep this request_id. Inspect rejection/transport; do not submit a new task ID.'};
  return {content:[{type:'text' as const,text:JSON.stringify(result)}],details:{project_id:b.project_id,read_only:p.action==='status'}};
 }});
}
