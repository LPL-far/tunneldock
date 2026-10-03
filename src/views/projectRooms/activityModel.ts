import type { ActivityPhase, ProjectActivity, TaskActivity } from '../../types/activity';
export function phaseStep(phase: ActivityPhase): number {
  switch (phase) {
    case 'queued': return 0;
    case 'running': return 1;
    case 'received': case 'missing_evidence': case 'contract_checked': return 2;
    case 'reviewing': case 'reviewed': case 'finalizing': return 3;
    default: return -1;
  }
}
export function duration(since: string | null, until: number): string {
  const start = since ? Date.parse(since) : NaN;
  if (!Number.isFinite(start) || until < start) return '—';
  const seconds = Math.floor((until - start) / 1000);
  return seconds < 60 ? `${seconds}s` : seconds < 3600 ? `${Math.floor(seconds / 60)}m ${seconds % 60}s` : `${Math.floor(seconds / 3600)}h ${Math.floor(seconds % 3600 / 60)}m`;
}
export function stateSignature(activity: ProjectActivity | null): string {
  return JSON.stringify(activity?.cards.map(c => [c.current?.task_id, c.current?.run_id, c.current?.task_status,
    c.current?.phase, c.current?.web_reviewed, c.current?.updated_at, c.queued]) ?? []);
}
export function reviewPrompt(cwd: string, tasks: TaskActivity[], locale: string): string {
  const receipt = tasks.filter(t => t.handoff_available && t.run_id).map(t => `${t.agent_id}: task_id=${t.task_id}, run_id=${t.run_id}`).join('\n');
  if (locale.toLowerCase().startsWith('zh')) return `继续项目 ${cwd}。\n以下结果已经收件，不要再次派发相同任务：\n${receipt}\n先读取 .tunneldock/web_status.json 和 agent_activity.json，确认仍是最新 run。对可审核结果提交 review.started（author=chatgpt、task_id、run_id），完整分段读取 handoff 并复核关键代码/测试。审核完成后提交 task.review（携带 run_id；accept/revise/block），咨询使用 consult.reviewed；需要持久记忆时完成 memory/cleanup。只在审核完成后反馈结论，不把 worker 完成当成审核通过。若本轮已打回重跑，不要使用旧 handoff。`;
  return `Continue ${cwd}. Results received; do not redispatch:\n${receipt}\nRead web_status.json and agent_activity.json; verify each current run, emit review.started with task_id/run_id, read full handoffs in bounded pages and check decisive source/test evidence. Emit task.review with run_id (or consult.reviewed). Complete required memory/cleanup. Report a conclusion only after review; never accept an older retry's handoff.`;
}
