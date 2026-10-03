import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from '../../i18n';
import { campaignCopy, campaignStateLabel, campaignActionLabel } from './campaignLabels';
import type { CampaignStatus, CampaignSummary } from '../../types/activity';

const button = 'rounded border border-zinc-600 px-2 py-1 text-xs text-zinc-100 hover:border-sky-400 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-400 disabled:opacity-40';

type Props = { status?: CampaignStatus; projectId: string };
export function CampaignPanel(props: Props) { return <CampaignSession key={props.projectId} {...props}/>; }
function CampaignSession({ status, projectId }: Props) {
  const { locale } = useTranslation();
  const tr = (text: string) => campaignCopy(text, locale);
  const label = (text: string) => campaignStateLabel(text, locale);
  const request = useRef(0);
  const [details, setDetails] = useState<CampaignStatus | null>(null);
  const shown = details && details.revision === status?.revision && details.room_enabled === status?.room_enabled ? details : status;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [draft, setDraft] = useState('');
  const [review, setReview] = useState<{ id: string; hash: string; text: string } | null>(null);
  const [reference, setReference] = useState('');
  useEffect(() => () => { request.current++; }, []);
  const control = async (event: Record<string, unknown>) => {
    const id = ++request.current;
    setBusy(true); setError(''); setNotice('');
    try {
      await invoke('campaign_control', { projectId, event });
      if (request.current !== id) return;
      setNotice(tr('Saved. Status updates with Project Room polling.'));
      setReview(null); setReference('');
    } catch (e) { if (request.current === id) setError(String(e)); }
    finally { if (request.current === id) setBusy(false); }
  };
  const inspect = async (c: CampaignSummary) => {
    const id = ++request.current;
    setBusy(true); setError(''); setReview(null); setReference('');
    try {
      const result = await invoke<{ plan: unknown; record: unknown; plan_sha256: string }>('campaign_plan', { projectId, campaignId: c.id });
      if (request.current !== id) return;
      if (result.plan_sha256 !== c.plan_sha256) throw new Error(tr('Plan hash changed; refresh before authorizing.'));
      setReview({ id: c.id, hash: result.plan_sha256, text: JSON.stringify({ plan: result.plan, record: result.record }, null, 2) });
    } catch (e) { if (request.current === id) setError(String(e)); }
    finally { if (request.current === id) setBusy(false); }
  };
  const loadDetails = async () => {
    const id = ++request.current; setBusy(true); setError('');
    try { const result = await invoke<CampaignStatus>('campaign_details', { projectId }); if (request.current === id) setDetails(result); }
    catch (e) { if (request.current === id) setError(String(e)); }
    finally { if (request.current === id) setBusy(false); }
  };
  return <section aria-label={tr('Research campaigns')} className="mt-3 rounded-xl border border-zinc-700 p-3 text-xs text-zinc-300">
    <h4 className="font-semibold text-zinc-100">{tr('Research campaigns')}</h4>
    <p className="mt-1 text-zinc-400">{tr('Protocol verification and scientific gate results remain separate from Web acceptance.')}</p>
    {!status && <p role="status" className="mt-2">{tr('Waiting for campaign status.')}</p>}
    {status && !status.error && status.items.length === 0 && <p className="mt-2">{tr('No recorded campaigns.')}</p>}
    {!!status?.total && <button className={`${button} mt-2`} disabled={busy} onClick={() => void loadDetails()}>{tr('Load full status on demand')} ({shown?.items.length ?? 0} / {status.total})</button>}
    {(status?.error || error) && <p role="alert" className="mt-2 break-words text-rose-300">{status?.error || error}</p>}
    {notice && <p role="status" className="mt-2 text-sky-300">{notice}</p>}
    {shown?.items.map(c => <article key={c.id} className="mt-3 border-t border-zinc-800 pt-3">
      <p className="break-words font-medium text-zinc-100">{c.question}</p>
      <p className="mt-1">{c.id} · {c.stage} · {label(c.state)}{c.paused ? tr(' · paused') : ''}{c.room_enabled === false ? tr(' · room disabled') : ''}</p>
      <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 sm:grid-cols-[auto_1fr_auto_1fr]">
        <dt>{tr('Execution')}</dt><dd>{c.execution ? label(c.execution) : tr('not submitted')}</dd>
        <dt>{tr('Evidence')}</dt><dd>{c.evidence ? label(c.evidence) : tr('not checked')}</dd>
        <dt>{tr('Gate')}</dt><dd>{c.gate === null ? tr('unknown') : c.gate ? 'PASS' : 'FAIL'}{c.metric !== null ? ` (${c.metric})` : ''}</dd>
        <dt>{tr('Web review')}</dt><dd>{c.web_reviewed ? c.accepted ? tr('reviewed / accepted') : tr('reviewed / not accepted') : tr('pending / not accepted')}</dd>
        <dt>{tr('Attempts / budget')}</dt><dd>{c.attempts} / {c.budget}</dd>
        <dt>{tr('Deadline')}</dt><dd className="break-all">{c.deadline}</dd>
      </dl>
      {c.active_drain && <p className="mt-2 text-amber-200">{tr('Active or uncertain submission is draining. Pause/deadline does not terminate processes.')}</p>}
      {c.reason && <p className="mt-2 break-words text-amber-200">{c.reason}</p>}
      {!!c.scientific_failures && <p className="mt-2 text-amber-200">{tr('Recorded scientific failures:')} {c.scientific_failures}. {tr('A later diagnostic does not erase them.')}</p>}
      <p className="mt-2">{tr('Next:')} {label(c.next_owner)} — {campaignActionLabel(c.next_action, locale)}</p>
      <details className="mt-2"><summary className="cursor-pointer">{tr('Plan and evidence identity')}</summary><code className="block break-all">{c.plan_sha256}</code>{c.evidence_directory && <code className="mt-1 block break-all">{c.evidence_directory}</code>}{(c.evidence_ids ?? []).map(id => <code key={id} className="mt-1 block break-all">{id}</code>)}</details>
      <div className="mt-2 flex flex-wrap gap-2">
        <button className={button} disabled={busy} onClick={() => void inspect(c)}>{c.authorization_pending ? tr('Review draft for authorization') : tr('Inspect frozen plan')}</button>
        {!c.authorization_pending && !c.web_reviewed && <button className={button} disabled={busy} onClick={() => void control({ kind: c.paused ? 'campaign.resume' : 'campaign.pause', campaign_id: c.id, plan_sha256: c.plan_sha256 })}>{c.paused ? tr('Resume scheduling') : tr('Pause scheduling')}</button>}
      </div>
      {review?.id === c.id && <div className="mt-2">
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded bg-zinc-950 p-2">{review.text}</pre>
        {c.authorization_pending && <><label className="mt-2 block">{tr('User authorization reference')}<input className="mt-1 block w-full rounded border border-zinc-600 bg-zinc-950 p-2 text-zinc-100" value={reference} maxLength={1000} onChange={e => setReference(e.target.value)} placeholder={tr('Reference to your approval of this exact plan')}/></label>
          <button className={`${button} mt-2`} disabled={busy || !reference.trim() || review.hash !== c.plan_sha256} onClick={() => void control({ kind: 'campaign.authorize', campaign_id: c.id, plan_sha256: review.hash, authorization_ref: reference.trim() })}>{tr('Authorize this exact plan and enable bounded scheduling')}</button></>}
      </div>}
    </article>)}
    <details className="mt-3"><summary className="cursor-pointer">{tr('Create draft from approved-plan JSON')}</summary>
      <p className="my-2">{tr('Use the version 1 plan schema in docs/AUTO_RESEARCH_CAMPAIGNS.md. A draft cannot dispatch.')}</p>
      <label>{tr('Plan JSON')}<textarea value={draft} onChange={e => setDraft(e.target.value)} maxLength={131072} rows={5} className="mt-1 block w-full rounded border border-zinc-600 bg-zinc-950 p-2 font-mono text-zinc-100"/></label>
      <button className={`${button} mt-2`} disabled={busy || !!status?.error || !draft.trim()} onClick={() => { try { const plan: unknown = JSON.parse(draft); void control({ kind: 'campaign.draft', plan }); } catch (e) { setError(String(e)); } }}>{tr('Save draft')}</button>
    </details>
  </section>;
}
