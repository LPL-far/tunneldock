// Admission guard and durable metadata, not a browser transport repair.
import type { ExtensionAPI, ExtensionContext, ToolDefinition } from '@earendil-works/pi-coding-agent';
import { closeSync, existsSync, fsyncSync, lstatSync, mkdirSync, openSync, readFileSync, readSync, realpathSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, randomUUID } from 'node:crypto';

const VERSION = 1;
const MAX_CHECKPOINT = 8192;
const MAX_ASSOCIATIONS = 1024;
const RECOVER = 'tunneldock_recover';
const ID = /^[A-Za-z0-9_-]{1,128}$/;
const REQUEST = /^(?:wfr_[0-9a-f]{32}|(?:wfr_)?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/i;
const source = fileURLToPath(import.meta.url);
const fingerprint = createHash('sha256').update(readFileSync(source)).digest('hex');
interface Policy { max_calls: number; max_output_bytes: number; max_elapsed_ms: number }
interface Checkpoint {
  version: number; project_id: string; session_id: string; request_id: string;
  calls: number; output_bytes: number; started_at: number; timestamp: string;
  last_call_id: string | null; last_call_name: string | null; last_error: boolean;
  pending: string[]; finished: string[];
  last_error_text?: string | null; last_error_text_omitted?: boolean;
}
type RecordValue = Record<string, unknown>;
function object(value: unknown): RecordValue {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw Error('Expected JSON object');
  return value as RecordValue;
}
function missing(error: unknown) { return (error as NodeJS.ErrnoException).code === 'ENOENT'; }
function problem(error: unknown): string {
  const e = error as NodeJS.ErrnoException;
  return `${e.code ? `stream I/O ${e.code}: ` : ''}${String(e.message ?? error)}`;
}
function noLink(path: string) {
  if (lstatSync(path).isSymbolicLink()) throw Error('Symlinked stream path refused');
}
function readJSON(path: string, limit = MAX_CHECKPOINT): RecordValue {
  noLink(path);
  const fd = openSync(path,'r');
  try {
    const buffer = Buffer.alloc(limit+1);
    let bytes = 0;
    while (bytes < buffer.length) {
      const n = readSync(fd,buffer,bytes,buffer.length-bytes,null);
      if (!n) break;
      bytes += n;
    }
    if (bytes > limit) throw Error('Stream JSON exceeds size limit');
    return object(JSON.parse(buffer.subarray(0,bytes).toString('utf8')));
  } finally { closeSync(fd); }
}
function normalized(path: string) {
  const full = resolve(path);
  return process.platform === 'win32' ? full.toLowerCase() : full;
}
function sameCwd(a: unknown, b: string) {
  return typeof a === 'string' && isAbsolute(a) && normalized(a) === normalized(b)
    && normalized(realpathSync(a)) === normalized(realpathSync(b));
}
function identity(ctx: ExtensionContext) {
  if (!sameCwd(ctx.sessionManager.getCwd(),ctx.cwd)) throw Error('Session cwd mismatch');
  const dir = join(ctx.cwd,'.tunneldock'); noLink(dir);
  const binding = readJSON(join(dir,'session_binding.json'));
  if (!sameCwd(binding.cwd,ctx.cwd)) throw Error('Project binding cwd mismatch');
  if (typeof binding.project_id !== 'string' || !ID.test(binding.project_id)) throw Error('Invalid project ID');
  const session = ctx.sessionManager.getSessionId();
  if (!ID.test(session)) throw Error('Invalid session ID');
  return {dir,project:binding.project_id,session};
}
function policy(cwd: string): Policy | null {
  const dir = join(cwd,'.tunneldock');
  try { noLink(dir); } catch(e) { if (missing(e)) return null; throw e; }
  let data: RecordValue;
  try { data = readJSON(join(dir,'stream-policy.json')); } catch(e) { if (missing(e)) return null; throw e; }
  if (data.enabled !== true) return null;
  const limits = {max_calls:[20,100],max_output_bytes:[98304,16777216],max_elapsed_ms:[300000,3600000]};
  const result = {} as Policy;
  for (const key of Object.keys(limits) as (keyof Policy)[]) {
    const value = data[key] ?? limits[key][0];
    if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 1 || value > limits[key][1]) throw Error(`Invalid ${key}`);
    result[key] = value;
  }
  return result;
}
function requestFor(ctx: ExtensionContext, call: string): string | null {
  let id = ctx.sessionManager.getLeafId();
  for (let depth=0; id && depth<64; depth++) {
    const entry = ctx.sessionManager.getEntry(id);
    if (!entry) break;
    if (entry.type === 'message' && entry.message.role === 'assistant') {
      const message = entry.message;
      if (message.content.some(part => part.type === 'toolCall' && part.id === call)) {
        const chappie = (message as typeof message & {chappie?:{requestId?:unknown}}).chappie;
        const request = chappie?.requestId;
        if (typeof request !== 'string' || request.length > 256) return null;
        const pieces = request.split('/');
        return pieces.length === 2 && pieces[1].length > 0 && REQUEST.test(pieces[0]) ? pieces[0] : null;
      }
    }
    id = entry.parentId;
  }
  return null;
}
function checkpointPath(dir: string, request: string) { return join(dir,'web_turns',request+'.json'); }
function outputDir(dir: string, create: boolean) {
  noLink(dir);
  const turns = join(dir,'web_turns');
  if (create && !existsSync(turns)) mkdirSync(turns);
  noLink(turns);
  if (!lstatSync(turns).isDirectory()) throw Error('Stream output is not a directory');
}
function atomic(dir: string, path: string, data: unknown) {
  noLink(dir);
  if (existsSync(path)) noLink(path);
  const text = JSON.stringify(data);
  if (Buffer.byteLength(text) > MAX_CHECKPOINT) throw Error('Checkpoint exceeds 8 KiB');
  const temp = path+'.'+randomUUID()+'.tmp';
  let created = false;
  try {
    const fd = openSync(temp,'wx',0o600); created = true;
    try { writeFileSync(fd,text); fsyncSync(fd); } finally { closeSync(fd); }
    noLink(dir); renameSync(temp,path); created = false;
  } finally {
    if (created) { try { unlinkSync(temp); } catch(e) { if (!missing(e)) console.error('Stream temporary-file cleanup failed:',problem(e)); } }
  }
}
function load(ctx: ExtensionContext, request: string) {
  const binding = identity(ctx);
  const path = checkpointPath(binding.dir,request);
  let c: Checkpoint;
  try {
    outputDir(binding.dir,false);
    const data = readJSON(path);
    const hashes = (v:unknown): v is string[] => Array.isArray(v) && v.length <= 100 && v.every(s=>typeof s==='string' && /^[a-f0-9]{32}$/.test(s));
    if (data.version !== VERSION || data.project_id !== binding.project || data.request_id !== request
      || typeof data.session_id !== 'string' || !ID.test(data.session_id)
      || !Number.isSafeInteger(data.calls) || Number(data.calls)<0 || Number(data.calls)>100
      || !Number.isSafeInteger(data.output_bytes) || Number(data.output_bytes)<0
      || !Number.isSafeInteger(data.started_at) || Number(data.started_at)<0 || Number(data.started_at)>Date.now()
      || !hashes(data.pending) || !hashes(data.finished) || data.pending.length+data.finished.length !== data.calls
      || new Set([...data.pending,...data.finished]).size !== data.calls
      || typeof data.timestamp !== 'string' || !Number.isFinite(Date.parse(data.timestamp))
      || !(data.last_call_id === null || typeof data.last_call_id === 'string' && ID.test(data.last_call_id))
      || !(data.last_call_name === null || typeof data.last_call_name === 'string' && ID.test(data.last_call_name))
      || typeof data.last_error !== 'boolean'
      || !(data.last_error_text == null || typeof data.last_error_text === 'string')
      || !(data.last_error_text_omitted === undefined || typeof data.last_error_text_omitted === 'boolean')) throw Error('Invalid stream checkpoint; budget not reset');
    // Select only metadata fields; never carry arbitrary disk fields into the next write.
    c = {version:VERSION,project_id:binding.project,session_id:data.session_id,request_id:request,
      calls:Number(data.calls),output_bytes:Number(data.output_bytes),started_at:Number(data.started_at),timestamp:data.timestamp,
      last_call_id:data.last_call_id,last_call_name:data.last_call_name,last_error:data.last_error,pending:data.pending,finished:data.finished,
      last_error_text:data.last_error_text as string | null | undefined,last_error_text_omitted:data.last_error_text_omitted as boolean | undefined};
  } catch(e) {
    if (!missing(e)) throw e;
    c = {version:VERSION,project_id:binding.project,session_id:binding.session,request_id:request,calls:0,output_bytes:0,
      started_at:Date.now(),timestamp:new Date().toISOString(),last_call_id:null,last_call_name:null,last_error:false,pending:[],finished:[]};
  }
  return {binding,path,c};
}
function save(state: ReturnType<typeof load>) {
  outputDir(state.binding.dir,true);
  state.c.timestamp = new Date().toISOString();
  const dir = join(state.binding.dir,'web_turns');
  if (Buffer.byteLength(JSON.stringify(state.c)) > MAX_CHECKPOINT && state.c.last_error_text) {
    state.c.last_error_text = null; state.c.last_error_text_omitted = true;
  }
  atomic(dir,state.path,state.c);
  atomic(dir,join(dir,'latest.json'),{request_id:state.c.request_id,project_id:state.binding.project});
}
function callHash(call: string) { return createHash('sha256').update(call).digest('hex').slice(0,32); }
function counters(c: Checkpoint) { return {calls:c.calls,output_bytes:c.output_bytes,elapsed_ms:Math.max(0,Date.now()-c.started_at)}; }
function checkpointResponse(path: string) {
  return {block:true,reason:`Turn allowance reached. Checkpoint: ${path}. Use tunneldock_recover once, then send a short progress reply and continue on a new user turn. Workers remain running.`};
}
function summaries(value: unknown, fields: string[]) {
  if (!Array.isArray(value)) throw Error('Invalid web_status summary');
  return value.slice(0,8).map(item=>{
    const row = object(item), summary: RecordValue = {};
    for (const key of fields) {
      const v = row[key];
      if (typeof v === 'string') summary[key] = v;
      else if (typeof v === 'boolean' || typeof v === 'number' && Number.isFinite(v) || v === null) summary[key] = v;
    }
    return summary;
  });
}
function recover(ctx: ExtensionContext, call: string, failure: string | null, requested?: string) {
  const binding = identity(ctx);
  const status = readJSON(join(binding.dir,'web_status.json'),1048576);
  if (status.project_id !== binding.project || !sameCwd(object(status.session_binding).cwd,ctx.cwd)) throw Error('web_status project/cwd mismatch');
  const currentRequest = requestFor(ctx,call);
  if (requested !== undefined && !REQUEST.test(requested)) throw Error('Invalid checkpoint request ID');
  const request = requested ?? currentRequest;
  const checkpointExists = !!request && existsSync(checkpointPath(binding.dir,request));
  if (requested && !checkpointExists) throw Error('Requested checkpoint not found; no work replayed');
  const state = request && checkpointExists ? load(ctx,request) : null;
  const result = {project_id:binding.project,session_id:binding.session,request_id:request,current_request_id:currentRequest,tracking:currentRequest?'tracked':'untracked',checkpoint_exists:checkpointExists,
    checkpoint_session_id:state?.c.session_id ?? null,
    execution_state:failure || state?.c.pending.length ? 'EXECUTION_UNCERTAIN' : state?.c.finished.length ? 'RESULT_OBSERVED' : 'NO_RESERVATION_OBSERVED',
    pending_count:state?.c.pending.length ?? 0,finished_count:state?.c.finished.length ?? 0,
    // Retain the explicit read-only receipt contract used by existing callers.
    pending_calls:state?.c.pending.length ?? 0,finished_calls:state?.c.finished.length ?? 0,
    execution_replay:false,browser_stream_recovered:false,
    last_call_id:state?.c.last_call_id ?? null,last_call_name:state?.c.last_call_name ?? null,last_error:state?.c.last_error ?? null,
    last_error_text:state?.c.last_error_text ?? null,last_error_text_omitted:state?.c.last_error_text_omitted ?? false,
    checkpoint_path:state?.path ?? null,counters:state ? counters(state.c) : null,error:failure,
    state_token:typeof status.state_token === 'string' ? status.state_token.slice(0,128) : null,
    tasks:summaries(status.tasks,['id','owner','status','review_mode','reviewed_by','updated_at','summary','error_message']),
    // web_status carries active run IDs on transports, not historical run receipts.
    runs:summaries(status.transports,['agent_id','status','active_run_id','error_message']),
    actions:summaries(status.actions,['priority','kind','task_id','consultation_id','handoff_path','finalizer_path']),
    limits:{items_per_section:8,max_bytes:8192},
    omitted_items:[status.tasks,status.transports,status.actions].reduce<number>((n,v)=>n+(Array.isArray(v)?Math.max(0,v.length-8):0),0),
    error_omitted:false,checkpoint_path_omitted:false,
    accounting_boundary:'Observed hook content (UTF-8 text/base64 plus JSON metadata/details); order dependent, not raw network bytes or a response ceiling.',
    next_action:'Read-only snapshot. Select older checkpoints only with explicit request_id. EXECUTION_UNCERTAIN requires receipt/artifact reconciliation before any retry; never assume safe replay. RESULT_OBSERVED is not scientific acceptance. New call IDs are not deduplicated. Omitted evidence remains in source files. Browser transport and upstream 429 are not repaired; workers remain running.'};
  while (Buffer.byteLength(JSON.stringify(result)) > MAX_CHECKPOINT) {
    const longest = [result.tasks,result.runs,result.actions].sort((a,b)=>b.length-a.length)[0];
    if (longest.length) { longest.pop(); result.omitted_items++; }
    else if (result.last_error_text) { result.last_error_text=null; result.last_error_text_omitted=true; }
    else if (result.error) { result.error=null; result.error_omitted=true; }
    else if (result.checkpoint_path) { result.checkpoint_path=null; result.checkpoint_path_omitted=true; }
    else throw Error('Recovery metadata exceeds output budget');
  }
  return result;
}

export default function stream(pi: ExtensionAPI) {
  let persistenceFailure: string | null = null;
  // Never evict an unresolved admission to make room for newer calls.
  const admitted = new Map<string,{request:string; name:string; scope:string; hash:string}>();
  const key = (ctx:ExtensionContext,call:string) => JSON.stringify([normalized(ctx.cwd),ctx.sessionManager.getSessionId(),call]);
  const failurePath = (b:ReturnType<typeof identity>) => join(b.dir,`stream-failure-${b.session}.json`);
  function failure(ctx:ExtensionContext): string | null {
    if (persistenceFailure) return persistenceFailure;
    const b=identity(ctx);
    try {
      const record=readJSON(failurePath(b));
      if (record.project_id!==b.project || record.session_id!==b.session || record.latched!==true || typeof record.reason!=='string') throw Error('Invalid failure latch; manual reconciliation required');
      return `Persisted admission failure latch: ${record.reason}${record.reason_omitted ? ' Full reason omitted due to metadata budget.' : ''}`;
    } catch(e) { if (missing(e)) return null; throw e; }
  }
  function latch(ctx:ExtensionContext,error:unknown) {
    const reason=`Stream accounting failed: ${problem(error)}. EXECUTION_UNCERTAIN; further admissions blocked. Reconcile receipts/artifacts; no automatic reset or replay. Workers remain running.`;
    persistenceFailure=reason;
    try {
      const b=identity(ctx);
      const omitted=Buffer.byteLength(reason)>4096;
      atomic(b.dir,failurePath(b),{version:VERSION,project_id:b.project,session_id:b.session,latched:true,
        reason:omitted?'Accounting failure; full reason exceeds metadata budget.':reason,reason_omitted:omitted,timestamp:new Date().toISOString()});
    } catch(e) {
      persistenceFailure+=` Failure latch could not be persisted: ${problem(e)}. Protection is memory-only and cannot be guaranteed after reload.`;
    }
    ctx.ui.notify(persistenceFailure,'error');
    return persistenceFailure;
  }
  pi.on('session_start',async(_event,ctx)=>{
    if (!existsSync(join(ctx.cwd,'.tunneldock/session_binding.json'))) return;
    try {
      const b = identity(ctx);
      atomic(b.dir,join(b.dir,'stream-capability.json'),{version:VERSION,fingerprint,loaded_file:source,pid:process.pid,
        session_id:b.session,project_id:b.project,loaded_at:new Date().toISOString(),enabled:!!policy(ctx.cwd),tool:RECOVER});
    } catch(e) { ctx.ui.notify(problem(e),'error'); }
  });
  pi.on('tool_call',async(event,ctx)=>{
    if (event.toolName === RECOVER) return;
    let reserving=false;
    try {
      const limits = policy(ctx.cwd); if (!limits) return;
      const failed=failure(ctx); if (failed) return {block:true,reason:failed};
      identity(ctx);
      if (admitted.has(key(ctx,event.toolCallId))) return {block:true,reason:'Tool call already reserved in this runtime; no replay.'};
      const request = requestFor(ctx,event.toolCallId);
      if (!request) return {block:true,reason:'Stream guard untracked: current toolCallId has no validated request identity. Send a short checkpoint reply; do not guess another client request.'};
      if (!ID.test(event.toolCallId) || !ID.test(event.toolName)) throw Error('Invalid tool call metadata');
      const state = load(ctx,request), c = state.c;
      const hash = callHash(event.toolCallId);
      if (c.pending.some(h=>![...admitted.values()].some(a=>a.request===request && a.scope===key(ctx,'') && a.hash===h))) {
        return {block:true,reason:'EXECUTION_UNCERTAIN: unresolved reservation from outside this admission map. Reconcile receipts/artifacts before continuing; no automatic reset.'};
      }
      if (c.calls >= limits.max_calls || c.output_bytes >= limits.max_output_bytes || counters(c).elapsed_ms >= limits.max_elapsed_ms) return checkpointResponse(state.path);
      if (c.pending.includes(hash) || c.finished.includes(hash)) return {block:true,reason:'Tool call already reserved; use tunneldock_recover to inspect continuity. No replay.'};
      if (admitted.size >= MAX_ASSOCIATIONS) return {block:true,reason:latch(ctx,Error('Admission identity map full; unresolved reservations are retained'))};
      c.calls++; c.pending.push(hash);
      reserving=true;
      save(state);
      admitted.set(key(ctx,event.toolCallId),{request,name:event.toolName,scope:key(ctx,''),hash});
    } catch(e) { return {block:true,reason:reserving ? latch(ctx,e) : problem(e)}; }
  });
  pi.on('tool_result',async(event,ctx)=>{
    if (event.toolName === RECOVER) return; // Recovery is read-only, including its result hook.
    try {
      const association=admitted.get(key(ctx,event.toolCallId));
      if (!association && !policy(ctx.cwd)) return;
      const request = association?.request ?? requestFor(ctx,event.toolCallId);
      if (!request) throw Error('Unresolved tool result identity; output accounting unavailable');
      const state = load(ctx,request), c = state.c, hash = callHash(event.toolCallId);
      if (c.finished.includes(hash)) return;
      if (!association || association.name!==event.toolName || !c.pending.includes(hash)) throw Error('Tool result has no exact in-process admission/reservation association');
      c.pending = c.pending.filter(id=>id!==hash); c.finished.push(hash);
      // Includes JSON metadata/escaping and base64 as observed here, not decoded image or wire bytes.
      const bytes = Buffer.byteLength(JSON.stringify({content:event.content,details:event.details,isError:event.isError}),'utf8');
      c.output_bytes = Math.min(Number.MAX_SAFE_INTEGER,c.output_bytes+bytes);
      if (!ID.test(event.toolCallId) || !ID.test(event.toolName)) throw Error('Invalid tool result metadata');
      c.last_call_id = event.toolCallId; c.last_call_name = event.toolName; c.last_error = event.isError;
      const errorText=event.isError ? event.content.filter(p=>p.type==='text').map(p=>p.text).join('\n') : null;
      c.last_error_text=errorText; c.last_error_text_omitted=false;
      save(state);
      admitted.delete(key(ctx,event.toolCallId));
    } catch(e) {
      const message=latch(ctx,e);
      return {content:[...event.content,{type:'text' as const,text:message}]};
    }
  });
  pi.registerTool({name:RECOVER,label:'Read stream continuity',description:'Read bounded current web_status and this exact Web request checkpoint. No replay, submission, review, worker control or browser recovery.',
    parameters:{type:'object',properties:{request_id:{type:'string',maxLength:64}},additionalProperties:false} as ToolDefinition['parameters'],
    async execute(id,params,_signal,_update,ctx) {
      const requested = (params as {request_id?:unknown}).request_id;
      if (requested !== undefined && typeof requested !== 'string') throw Error('Invalid request_id');
      let failed:string|null;
      try { failed=failure(ctx); } catch(e) { failed=problem(e); }
      const result = recover(ctx,id,failed,requested);
      return {content:[{type:'text' as const,text:JSON.stringify(result)}],details:{read_only:true}};
    }
  });
}
