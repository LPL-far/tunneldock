export type ActivityPhase = 'queued' | 'running' | 'received' | 'reviewing' | 'reviewed' | 'finalizing' | 'blocked' | 'superseded' | 'contract_checked' | 'missing_evidence' | 'closed_unverified' | 'unknown';
export interface TaskActivity {
  task_id: string; title: string; task_status: string; agent_id: string; kind: string;
  phase: ActivityPhase; attempts: number; run_id: string | null; run_status: string | null;
  thread_id: string | null; queued_at: string; updated_at: string; started_at: string | null;
  received_at: string | null; handoff_available: boolean; reviewed_by: string; web_reviewed: boolean;
  review_started_at: string | null; detail: string; error: string | null;
  progress: { units: number; event: string; last_event_at: string } | null;
}
export interface AgentActivityCard {
  agent_id: string; current: TaskActivity | null; queued: number; transport: string | null;
  human_thread_id: string | null; automation_thread_id: string | null;
}
export interface ProjectActivity {
  version: number; project_id: string; revision: string; sampled_at: string;
  cards: AgentActivityCard[]; pending_reviews: TaskActivity[]; pending_review_count: number;
}
export interface HandoffPage {
  run_id: string; path: string; offset: number; next_offset: number;
  total_bytes: number; complete: boolean; text: string;
}
