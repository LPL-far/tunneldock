import { useMemo, useState } from 'react';
import { useTranslation } from '../../i18n';
import { segmentHandoff } from './handoffReading';

export function HandoffReader({text,offset,complete}:{text:string;offset:number;complete:boolean}) {
  const { locale } = useTranslation();
  const zh = locale.startsWith('zh');
  const [raw,setRaw] = useState(false);
  const sections = useMemo(()=>segmentHandoff(text),[text]);
  const names:Record<string,string> = zh ? {
    outcome:'执行结果',summary:'发生了什么',files:'修改范围',verification:'做过哪些验证',
    evidence:'证据在哪里',cleanup:'清理记录',risks:'风险与阻塞',review:'需要审核什么',
    reasoning:'科研论证',memory:'记忆更新',contract:'机器回执（不是审核结论）',source:'原文片段',
  } : {
    outcome:'Outcome',summary:'What changed',files:'Changed files',verification:'Verification',
    evidence:'Evidence',cleanup:'Cleanup',risks:'Risks and blockers',review:'Review request',
    reasoning:'Research reasoning',memory:'Memory updates',contract:'Machine receipt (not a review)',source:'Source excerpt',
  };
  return <div>
    <div className="flex flex-wrap items-center gap-2" aria-label={zh?'阅读方式':'Reading mode'}>
      {[false,true].map(value=><button key={String(value)} type="button" aria-pressed={raw===value} onClick={()=>setRaw(value)}
        className={`rounded-md border px-2.5 py-1.5 text-xs focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400 ${raw===value?'border-sky-600 text-sky-200':'border-zinc-700 text-zinc-400'}`}>
        {value?(zh?'原文':'Original'):(zh?'分块阅读':'Structured reading')}
      </button>)}
    </div>
    <p className="my-2 text-[11px] leading-5 text-zinc-400">{zh?
      '按原文分块，不生成新结论；短句与固定术语仅是写作指导，不是 STE 合规认证。':
      'Source sections, not a new conclusion. Plain-writing guidance is not STE compliance certification.'}</p>
    {(offset>0 || !complete) && <p role="note" className="mb-2 text-xs text-amber-300">{zh?
      '这里只显示本页。其他页可能有关键条件、错误或反证；未显示不等于不存在。':
      'This is one page. Other pages may contain conditions, errors or counterevidence; absence here proves nothing.'}</p>}
    {raw?<pre className="max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-zinc-900/50 p-3 text-xs leading-6 text-zinc-300">{text}</pre>:
      <div className="max-h-96 space-y-2 overflow-auto pr-1">{sections.map((section,i)=><section key={i}
        className={`rounded-lg border p-3 ${section.kind==='risks'?'border-amber-900/60 bg-amber-950/10':'border-zinc-800 bg-zinc-900/40'}`}>
        <h5 className="mb-1.5 text-xs font-semibold text-zinc-200">{names[section.kind]??section.title}</h5>
        <pre className="whitespace-pre-wrap break-words text-xs leading-6 text-zinc-300">{section.raw}</pre>
      </section>)}</div>}
  </div>;
}
