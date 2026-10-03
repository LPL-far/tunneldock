// Wait for durable receipts inside one active Web turn. Never submit/review work.
import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

export interface WaitInput { task_ids: string[]; max_wait_seconds?: number }
interface Task { id: string; title?: string; status: string; owner?: string }
interface Run { id: string; task_id: string; status: string; started_at: string; output_path?: string }
interface Snapshot { tasks: Task[]; runs: Run[] }
export interface Receipt { task_id: string; status: string; agent: string | null; run_id: string | null; handoff: string | null; available: boolean }
export function receipts(snapshot: Snapshot, ids: string[]): Receipt[] {
  return ids.map(id => {
    const task = snapshot.tasks.find(t => t.id === id);
    const run = snapshot.runs.filter(r => r.task_id === id)
      .sort((a,b) => Date.parse(b.started_at) - Date.parse(a.started_at))[0];
    // A task requeued for revision must never consume its older completed run.
    const eligible = !!task && ['review','completed'].includes(task.status) && run?.status === 'completed';
    let available = false;
    if (eligible && run.output_path) {
      try { const s = statSync(run.output_path); available = s.isFile() && s.size > 0; } catch { /* receipt not yet flushed */ }
    }
    return { task_id: id, status: task?.status ?? 'not_in_snapshot', agent: task?.owner ?? null,
      run_id: task && !['queued','backlog'].includes(task.status) ? run?.id ?? null : null,
      handoff: available ? run.output_path! : null, available };
  });
}
export function receiptState(items: Receipt[]): string {
  if (items.some(r => ['blocked','failed','superseded'].includes(r.status))) return 'blocked';
  return items.length > 0 && items.every(r => r.available) ? 'ready_for_review' : 'waiting';
}
function pause(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise(resolve => {
    const finish = () => { clearTimeout(timer); signal?.removeEventListener('abort',finish); resolve(); };
    const timer = setTimeout(finish,ms);
    signal?.addEventListener('abort',finish,{once:true});
    if (signal?.aborted) finish();
  });
}
export async function waitForReceipts(cwd: string, input: WaitInput, signal?: AbortSignal) {
  if (!Array.isArray(input.task_ids) || input.task_ids.length < 1 || input.task_ids.length > 8
    || input.task_ids.some(id => typeof id !== 'string' || !/^TASK-[A-Za-z0-9_-]+$/.test(id))) {
    throw new Error('Provide 1–8 exact TASK IDs from the current Project Room.');
  }
  const binding = JSON.parse(readFileSync(join(cwd,'.tunneldock/session_binding.json'),'utf8'));
  if (typeof binding.cwd !== 'string' || !binding.project_id || resolve(binding.cwd).toLowerCase() !== resolve(cwd).toLowerCase()) {
    throw new Error('Project binding does not match this Pi cwd. Refusing cross-project wait.');
  }
  const seconds = input.max_wait_seconds ?? 60;
  if (!Number.isFinite(seconds) || seconds < 0 || seconds > 90) throw new Error('max_wait_seconds must be between 0 and 90.');
  const started = Date.now(), deadline = started + seconds * 1000;
  let items: Receipt[] = [], problem: string | null = null, state = 'waiting';
  do {
    if (signal?.aborted) { state = 'cancelled'; break; }
    try {
      const data = JSON.parse(readFileSync(join(cwd,'.tunneldock/project_room.json'),'utf8'));
      if (!Array.isArray(data.tasks) || !Array.isArray(data.runs)) throw new Error('Invalid coordination snapshot');
      items = receipts(data,input.task_ids); state = receiptState(items); problem = null;
      if (state !== 'waiting') break;
      if (items.some(r => r.status === 'not_in_snapshot')) { state = 'not_found'; break; }
    } catch (e) { problem = String(e).slice(0,320); }
    if (Date.now() >= deadline) { state = 'timeout'; break; }
    await pause(Math.min(2000, deadline-Date.now()),signal);
  } while (true);
  return { project_id: binding.project_id, state, elapsed_seconds: Math.round((Date.now()-started)/1000),
    receipts: items, error: problem,
    next_action: state === 'ready_for_review'
      ? 'Continue in THIS Web turn: confirm latest run IDs, emit review.started, read ALL handoff pages and decisive source/test evidence, submit task.review or consult.reviewed, then report the reviewed result. Receipt is NOT review.'
      : state === 'blocked'
      ? 'Inspect the explicit blocker and any returned evidence. Do not invent consensus or repeatedly dispatch another writer.'
      : 'No result is discarded. Read the durable state before resuming; timeout/cancellation does not cancel workers or imply task failure. Do not claim to have reviewed absent results.' };
}
// Existing Pi sessions can use the SAME implementation through bash/Node,
// without a session restart just to register the new native tool.
if (process.argv.includes('--td-wait')) {
  const index = process.argv.indexOf('--td-wait');
  Promise.resolve().then(() => waitForReceipts(process.cwd(), JSON.parse(process.argv[index+1] ?? '{}')))
    .then(result => console.log(JSON.stringify(result)))
    .catch(error => { console.error(String(error)); process.exitCode = 1; });
}

export default function (pi: ExtensionAPI) {
  pi.registerTool({
    name: 'tunneldock_wait', label: 'Wait for Agent receipts',
    description: 'Wait for 1–8 existing Project Room task IDs (not a new task). Use when the user asks to await Codex/Gemini then review before answering. Returns compact receipt paths; continue reviewing in the SAME active Web turn. Waits up to 90 seconds, supports cancellation, never prints logs, submits work, accepts results, or wakes an inactive browser.',
    // Plain JSON Schema is supported by Pi; no additional runtime dependency.
    parameters: { type:'object', required:['task_ids'], additionalProperties:false, properties:{
      task_ids:{type:'array',minItems:1,maxItems:8,items:{type:'string'}},
      max_wait_seconds:{type:'number',minimum:0,maximum:90,default:60}
    }} as any,
    async execute(_id: string, params: WaitInput, signal: AbortSignal | undefined, _onUpdate: unknown, ctx: {cwd:string}) {
      const result = await waitForReceipts(ctx.cwd,params,signal);
      return {content:[{type:'text' as const,text:JSON.stringify(result)}],details:{project_id:result.project_id,state:result.state}};
    }
  });
}
