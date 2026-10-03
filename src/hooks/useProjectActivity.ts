import { useEffect, useState } from 'react';
import { getProjectActivity } from '../api';
import type { ProjectActivity } from '../types/activity';

// One in-flight request; no interval overlap or snapshot from a previous project.
export function useProjectActivity(projectId: string | null) {
  const [value, setValue] = useState<ProjectActivity | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setValue(null); setError(null);
    if (!projectId) return;
    const poll = async () => {
      try {
        const next = await getProjectActivity(projectId);
        if (!cancelled && next.project_id === projectId) { setValue(next); setError(null); }
      } catch (e) { if (!cancelled) setError(String(e)); }
      finally { if (!cancelled) timer = setTimeout(() => void poll(), 2500); }
    };
    void poll();
    return () => { cancelled = true; clearTimeout(timer); };
  }, [projectId]);
  return { activity: value?.project_id === projectId ? value : null, error };
}
