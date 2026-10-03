export interface AutoCompactionState {
  state?: string; phase?: string; enabled?: boolean; checked_at?: string;
  changed_files?: unknown[]; error?: string; mode?: string;
}
export interface ContextEngineState {
  auto_compaction?: AutoCompactionState;
  phase: string; checked_at?: string; generated_at?: string; revision?: string;
  memory_source_bytes?: number; hot_bytes?: number; hot_budget_bytes?: number;
  indexed_code_files?: number; omitted_count?: number; hot_path?: string;
  canonical_modified?: boolean; error?: string; projection_reduction?: number;
}
export type ActivityPhase = 'queued' | 'running' | 'received' | 'reviewing' | 'reviewed' | 'finalizing' | 'blocked' | 'superseded' | 'contract_checked' | 'missing_evidence' | 'closed_unverified' | 'unknown';
export interface TaskActivity {
  task_id: string; title: string; task_status: string; agent_id: string; kind: string;
  phase: ActivityPhase; attempts: number; run_id: string | null; run_status: string | null;
  thread_id: string | null; queued_at: string; updated_at: string; started_at: string | null;
  received_at: string | null; handoff_available: boolean; reviewed_by: string; web_reviewed: boolean;
  review_started_at: string | null; detail: string; error: string | null;
  next_owner?: string;
  progress: { units: number; event: string; last_event_at: string } | null;
}
export interface AgentActivityCard {
  agent_id: string; current: TaskActivity | null; queued: number; transport: string | null;
  human_thread_id: string | null; automation_thread_id: string | null;
}
export interface ProjectActivity {
  context_engine?: ContextEngineState;
  version: number; project_id: string; revision: string; sampled_at: string;
  cards: AgentActivityCard[]; pending_reviews: TaskActivity[]; pending_review_count: number;
}
export interface HandoffPage {
  run_id: string; path: string; offset: number; next_offset: number;
  total_bytes: number; complete: boolean; text: string;
}
