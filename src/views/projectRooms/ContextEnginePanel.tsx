import { useState } from 'react';
import { BookOpen, Copy, Search } from 'lucide-react';
import { useTranslation } from '../../i18n';
import { invoke } from '@tauri-apps/api/core';

import type { ContextEngineState } from '../../types/activity';

interface Hit {path:string;line:number;text:string;sha256:string;current?:boolean;kind:string}
const size=(n:number|undefined)=>n===undefined?'—':`${(n/1024).toFixed(1)} KiB`;
export function ContextEnginePanel({state,cwd,projectId,now}:{state?:ContextEngineState;cwd:string;projectId:string;now:number}) {
  const {locale}=useTranslation();const zh=locale.startsWith('zh');
  const [query,setQuery]=useState(''),[results,setResults]=useState<Hit[]>([]),[busy,setBusy]=useState(false),[notice,setNotice]=useState('');
  const phases=zh?['扫描来源','生成工作集','可检索']:['Scan sources','Pack context','Searchable'];
  const phase=state?.phase||'not_indexed';const blocked=phase==='blocked'||phase==='unavailable';
  const stale=state?.checked_at?now-Date.parse(state.checked_at)>150000:false;
  const active=phase==='scanning'?0:phase==='packing'?1:phase==='ready'?2:-1;
  const labels:Record<string,string>=zh?{ready:'自动更新正常',scanning:'正在扫描',packing:'正在压缩',blocked:'压缩受阻',unavailable:'状态不可用',not_indexed:'等待首次索引'}:{ready:'Automatic refresh active',scanning:'Scanning',packing:'Packing',blocked:'Blocked',unavailable:'Unavailable',not_indexed:'Awaiting first index'};
  async function search(){
    if(!query.trim()||busy)return;setBusy(true);setNotice('');
    try{const r=await invoke<{hits:Hit[]}>('query_project_context',{projectId,action:'search',query:query.trim()});setResults(r.hits||[]);if(!r.hits?.length)setNotice(zh?'未找到匹配；这不证明资料不存在。':'No match in the bounded index; this is not proof of absence.');}
    catch(e){setNotice(String(e));}finally{setBusy(false);}
  }
  return <section className="mt-4 rounded-xl border border-zinc-800 bg-zinc-950/30 p-3" aria-label={zh?'上下文与自动记忆压缩':'Context and automatic memory packing'}>
    <div className="flex flex-wrap items-center justify-between gap-2">
      <h4 className="flex items-center gap-2 text-xs font-semibold text-zinc-200"><BookOpen size={14}/>{zh?'上下文与自动记忆压缩':'Context & memory packing'}</h4>
      <span role="status" className={`text-[11px] ${blocked||stale?'text-amber-300':'text-emerald-300'}`}>{stale?(zh?'状态较旧，等待同步':'Status stale; awaiting refresh'):labels[phase]||phase}</span>
    </div>
    <div className="mt-3 grid grid-cols-3 gap-2 text-[11px]">
      <div><p className="text-zinc-500">{zh?'记忆原文 → 默认工作集':'Memory source → working set'}</p><p className="mt-1 font-mono text-zinc-300">{size(state?.memory_source_bytes)} → {size(state?.hot_bytes)}</p></div>
      <div><p className="text-zinc-500">{zh?'本地代码索引':'Local code index'}</p><p className="mt-1 text-zinc-300">{state?.indexed_code_files??'—'} {zh?'个文件':'files'} · {zh?'词法候选':'lexical candidates'}</p></div>
      <div><p className="text-zinc-500">{zh?'最近检查':'Last checked'}</p><p className="mt-1 font-mono text-zinc-300">{state?.checked_at?new Date(state.checked_at).toLocaleTimeString(locale):'—'}</p></div>
    </div>
    <ol className="mt-3 grid grid-cols-3 gap-2" aria-label={zh?'实际生成阶段，不是预测百分比':'Observed phases, not estimated percentages'}>{phases.map((p,i)=><li key={p} className="text-[10px] text-zinc-500"><div className={`mb-1 h-0.5 ${i<=active&&!blocked?'bg-emerald-500/70':'bg-zinc-800'}`}/>{p}</li>)}</ol>
    <p className="mt-2 text-[10px] leading-5 text-zinc-500">{zh?'原文保留且可按哈希恢复；这是不完整的检索工作集，不会自动改写科研结论。代码关系仅供定位，不用于证明可删除。':'Originals retained by hash. This incomplete retrieval view never rewrites scientific conclusions. Code relations are navigation hints, not deletion evidence.'}</p>
    {!!state?.omitted_count&&<p className="text-[10px] text-amber-400">{zh?`有 ${state.omitted_count} 项因扫描预算或类型限制未索引。`:`${state.omitted_count} items excluded by scan budgets or file policy.`}</p>}
    {state?.error&&<p role="alert" className="mt-1 break-words text-xs text-amber-300">{state.error}</p>}
    <form className="mt-3 flex flex-wrap gap-2" onSubmit={e=>{e.preventDefault();void search();}}>
      <input aria-label={zh?'搜索记忆与代码':'Search memory and code'} value={query} maxLength={300} onChange={e=>setQuery(e.target.value)} placeholder={zh?'先找现有实现、约束或实验，再读取原文':'Find implementation, constraints or evidence before reading'} className="min-w-0 flex-1 rounded-lg border border-zinc-800 bg-zinc-950 px-2 py-1.5 text-xs text-zinc-200"/>
      <button disabled={busy||!query.trim()} className="flex items-center gap-1 rounded-lg border border-zinc-700 px-2 text-xs text-zinc-300 disabled:opacity-40"><Search size={12}/>{busy?'…':zh?'检索':'Search'}</button>
      {state?.hot_path&&<button type="button" onClick={()=>{navigator.clipboard.writeText(`${cwd.replace(/[\\/]$/,'')}/${state.hot_path}`).then(()=>setNotice(zh?'已复制工作集路径':'Working-set path copied'),e=>setNotice(String(e)));}} className="flex items-center gap-1 rounded-lg border border-zinc-700 px-2 text-xs text-zinc-300"><Copy size={12}/>{zh?'工作集路径':'Working-set path'}</button>}
    </form>
    {notice&&<p role="status" className="mt-2 break-words text-xs text-amber-300">{notice}</p>}
    {results.length>0&&<div className="mt-2 max-h-56 space-y-2 overflow-auto">{results.map(h=><div key={`${h.sha256}:${h.line}`} className="rounded-lg border border-zinc-800 p-2"><p className="break-all font-mono text-[10px] text-emerald-300">{h.path}:L{h.line}{h.current===false?(zh?' · 原文已变化':' · source changed'):''}</p><p className="mt-1 whitespace-pre-wrap break-words text-xs text-zinc-300">{h.text}</p><p className="mt-1 truncate font-mono text-[9px] text-zinc-600" title={h.sha256}>{h.sha256}</p></div>)}</div>}
  </section>;
}
