import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getProjectActivity } from '../api';
import type { ProjectActivity } from '../types/activity';

// Event-driven context updates plus polling fallback, with one request in flight.
export function useProjectActivity(projectId: string | null) {
  const [value, setValue] = useState<ProjectActivity | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false, inFlight = false, pending = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let unlisten: (() => void) | undefined;
    setValue(null); setError(null);
    if (!projectId) return;
    const poll = async () => {
      if (cancelled) return;
      if (inFlight) { pending = true; return; }
      clearTimeout(timer); inFlight = true; pending = false;
      try {
        const next = await getProjectActivity(projectId);
        if (!cancelled && next.project_id === projectId) { setValue(next); setError(null); }
      } catch (e) { if (!cancelled) setError(String(e)); }
      finally {
        inFlight = false;
        if (!cancelled) timer = setTimeout(() => void poll(), pending ? 100 : 2500);
      }
    };
    void listen<{project_id: string}>('project-context-updated', event => {
      if (event.payload.project_id === projectId) void poll();
    }).then(stop => { if (cancelled) stop(); else unlisten = stop; })
      .catch(e => { if (!cancelled) setError(`Live events unavailable; polling continues: ${String(e)}`); });
    void poll();
    return () => { cancelled = true; clearTimeout(timer); unlisten?.(); };
  }, [projectId]);
  return { activity: value?.project_id === projectId ? value : null, error };
}
