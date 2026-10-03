import { useEffect, useRef, useState } from 'react';
import { Activity, Bot, Check, Copy, FileText, LoaderCircle, Sparkles, X } from 'lucide-react';
import { useTranslation } from '../../i18n';
import type { AgentActivityCard, HandoffPage, ProjectActivity, TaskActivity } from '../../types/activity';
import { readProjectHandoff } from '../../api';
import { duration, phaseStep, reviewPrompt } from './activityModel';

const button = 'inline-flex items-center gap-1.5 rounded-lg border border-zinc-700 px-2.5 py-1.5 text-xs text-zinc-300 hover:border-sky-500 hover:text-white focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400 disabled:opacity-40';
export function AgentActivityPanel({ data, error, cwd }: { data: ProjectActivity | null; error: string | null; cwd: string }) {
  const { t, locale } = useTranslation();
  const [now, setNow] = useState(Date.now());
  const [selected, setSelected] = useState<TaskActivity | null>(null);
  const [page, setPage] = useState<HandoffPage | null>(null);
  const [offsets, setOffsets] = useState<number[]>([0]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const request = useRef(0);
  useEffect(() => { const id = setInterval(() => setNow(Date.now()), 1000); return () => clearInterval(id); }, []);
  useEffect(() => { request.current++; setSelected(null); setPage(null); setNotice(''); }, [data?.project_id]);
  useEffect(() => () => { request.current++; }, []);
  const copy = async (text: string) => {
    try { await navigator.clipboard.writeText(text); setNotice(t('activity.copied')); }
    catch (e) { setNotice(String(e)); }
  };
  const load = async (task: TaskActivity, offset: number, history: number[]) => {
    if (!task.run_id || !data) return;
    const id = ++request.current;
    setSelected(task); setPage(null); setOffsets(history); setBusy(true); setNotice('');
    try {
      const next = await readProjectHandoff(data.project_id, task.run_id, offset);
      if (id === request.current) setPage(next);
    } catch (e) { if (id === request.current) setNotice(String(e)); }
    finally { if (id === request.current) setBusy(false); }
  };
  const card = (agent: AgentActivityCard) => {
    const task = agent.current;
    const active = task?.phase === 'running';
    const step = task ? phaseStep(task.phase) : -1;
    const awaiting = task?.phase === 'received' || task?.phase === 'reviewing';
    const eventAt = task?.progress?.last_event_at ?? null;
    const quiet = active && eventAt && now - Date.parse(eventAt) > 180_000;
    const accent = agent.agent_id === 'codex' ? 'text-sky-300' : 'text-violet-300';
    const Icon = agent.agent_id === 'codex' ? Bot : Sparkles;
    return <article key={agent.agent_id} className="min-w-0 rounded-xl border border-zinc-800 bg-zinc-950/50 p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h4 className={`flex items-center gap-2 text-sm font-semibold ${accent}`}><Icon size={17}/>{agent.agent_id === 'codex' ? 'Codex' : 'Antigravity / Gemini'}</h4>
        <span className="rounded-md border border-zinc-800 bg-zinc-900 px-2 py-0.5 text-xs tabular-nums text-zinc-400">{t('activity.queue')} {agent.queued}</span>
      </div>
      <div role="status" className={`mt-3 flex items-center gap-2 text-xs ${task?.phase === 'blocked' ? 'text-rose-300' : awaiting ? 'text-amber-300' : 'text-zinc-300'}`}>
        {active ? <LoaderCircle size={13} className="animate-spin motion-reduce:animate-none"/> : task?.phase === 'reviewed' ? <Check size={13}/> : <span className="h-1.5 w-1.5 rounded-full bg-current"/>}
        {task ? t(`activity.phases.${task.phase}`) : t('activity.idle')}
      </div>
      <p className="mt-2 min-h-10 break-words text-sm leading-5 text-zinc-100">{task?.title ?? '—'}</p>
      <ol aria-label={t('activity.title')} className="mt-3 grid grid-cols-4 gap-1.5">
        {(['queue','execute','receive','review'] as const).map((key,i) => <li key={key} className="min-w-0">
          <div className={`h-1 rounded-full ${i <= step ? (agent.agent_id === 'codex' ? 'bg-sky-500/75' : 'bg-violet-500/75') : 'bg-zinc-800'}`}/>
          <div className={`mt-1.5 text-[11px] ${i === step ? 'text-zinc-200' : 'text-zinc-500'}`}>{t(`activity.${key}`)}{i === 3 && task?.phase === 'contract_checked' ? ' *' : ''}</div>
        </li>)}
      </ol>
      <dl className="mt-3 grid grid-cols-2 gap-x-2 gap-y-1 text-[11px]">
        <dt className="text-zinc-500">{awaiting ? t('activity.review_wait') : t('activity.elapsed')}</dt>
        <dd className="text-right font-mono text-zinc-300">{duration(awaiting ? task?.received_at ?? null : task?.started_at ?? null, !awaiting && task?.received_at ? Date.parse(task.received_at) : now)}</dd>
        <dt className="text-zinc-500">{t('activity.event_age')}</dt><dd className="text-right font-mono text-zinc-400">{duration(eventAt, now)}</dd>
      </dl>
      <p className={`mt-2 min-h-4 text-[11px] ${quiet ? 'text-amber-400' : 'text-zinc-500'}`}>{quiet ? t('activity.quiet') : task?.progress ? t('activity.units', {count: task.progress.units}) : active ? t('activity.no_events') : task ? t('activity.attempts', {count: task.attempts}) : ''}</p>
      {task?.error && <p className="mt-2 break-words text-xs text-rose-300">{task.error}</p>}
      {task?.phase === 'blocked' && task.detail && <p className="mt-2 line-clamp-3 text-xs text-rose-300/80" title={task.detail}>{task.detail}</p>}
      <details className="mt-3 text-[11px] text-zinc-500">
        <summary className="cursor-pointer py-1 hover:text-zinc-300">{t('activity.source')}</summary>
        {[[t('activity.human'),agent.human_thread_id],[t(agent.agent_id === 'codex' ? 'activity.automation' : 'activity.cascade'),task?.thread_id || agent.automation_thread_id]].filter(([,id]) => id).map(([label,id]) => <div key={label} className="mt-1 flex items-center gap-1.5"><span>{label}</span><code className="min-w-0 flex-1 truncate" title={id!}>{id}</code><button className="p-1 hover:text-white" title={t('activity.copy_id')} aria-label={t('activity.copy_id')} onClick={() => void copy(id!)}><Copy size={12}/></button></div>)}
        <div className="mt-1 break-all font-mono">{task?.run_id}</div>
      </details>
      {task?.handoff_available && <button type="button" className={`${button} mt-3`} onClick={() => void load(task,0,[0])}><FileText size={13}/>{t('activity.preview')}</button>}
    </article>;
  };
  return <section className="rounded-2xl border border-zinc-800/80 bg-dark-card p-4" aria-label={t('activity.title')}>
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div><h3 className="flex items-center gap-2 text-sm font-semibold text-zinc-200"><Activity size={16} className="text-emerald-400"/>{t('activity.title')}</h3><p className="mt-1 text-[11px] text-zinc-500">{t('activity.subtitle')}</p></div>
      {data && <span className="text-[10px] text-zinc-500">{t('activity.sampled')} {new Date(data.sampled_at).toLocaleTimeString(locale)}</span>}
    </div>
    {error && <div role="alert" className="my-2 text-xs text-amber-300" title={error}>{t('activity.sync_error')}</div>}
    <div className="mt-3 grid gap-3 xl:grid-cols-2">{data ? data.cards.map(card) : <p className="py-6 text-xs text-zinc-500">{t('activity.waiting')}</p>}</div>
    {data && data.pending_review_count > 0 && <div role="status" className="mt-3 rounded-lg border border-amber-900/50 bg-amber-950/15 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2"><span className="text-xs font-medium text-amber-200">{t('activity.pending',{count:data.pending_review_count})}</span>
        <button className={button} onClick={() => void copy(reviewPrompt(cwd,data.pending_reviews,locale))}><Copy size={13}/>{t('activity.prompt')}</button></div>
      {data.pending_reviews.slice(0,3).map(task => <div key={task.task_id} className="mt-2 flex items-center justify-between gap-2 text-xs text-zinc-400"><span className="truncate">{task.agent_id} · {task.title}</span><span className="shrink-0 font-mono text-amber-400/80">{duration(task.received_at,now)}</span></div>)}
    </div>}
    <p className="mt-3 text-[11px] leading-5 text-zinc-500">{t('activity.boundary')}</p>
    {notice && <p role="status" className="mt-2 break-words text-xs text-sky-300">{notice}</p>}
    {selected && <div className="mt-4 rounded-xl border border-zinc-700 bg-zinc-950 p-4">
      <div className="flex justify-between gap-3"><h4 className="text-sm text-zinc-100">{selected.title}</h4><button aria-label={t('activity.close')} className="text-zinc-400 hover:text-white" onClick={() => { request.current++; setSelected(null); }}><X size={17}/></button></div>
      <p className="my-2 text-xs text-amber-300">{t('activity.preview_note')}</p>
      {busy ? <LoaderCircle className="animate-spin motion-reduce:animate-none" size={16}/> : page && <>
        <pre className="max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-zinc-900/50 p-3 text-xs leading-6 text-zinc-300">{page.text}</pre>
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <button disabled={offsets.length < 2} className={button} onClick={() => void load(selected,offsets[offsets.length-2],offsets.slice(0,-1))}>{t('activity.previous')}</button>
          <span className="text-xs font-mono text-zinc-500">{page.offset}–{page.next_offset} / {page.total_bytes} B</span>
          <button disabled={page.complete} className={button} onClick={() => void load(selected,page.next_offset,[...offsets,page.next_offset])}>{t('activity.next')}</button>
          <button className={button} onClick={() => void copy(page.path)}>{t('activity.full_path')}</button>
        </div>
      </>}
    </div>}
  </section>;
}
