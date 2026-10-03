// Project-scoped output backpressure. No timers, processes, or model requests.
import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';

export const MAX_TEXT_BYTES = 8192;
export function utf8Head(text: string, limit: number): string {
  const bytes = Buffer.from(text);
  if (bytes.length <= limit) return text;
  let end = Math.max(0, limit);
  while (end > 0 && (bytes[end] & 0xc0) === 0x80) end--;
  return bytes.subarray(0, end).toString('utf8');
}
export function boundText(cwd: string, text: string): { text: string; path: string | null; bytes: number } {
  const bytes = Buffer.byteLength(text);
  if (bytes <= MAX_TEXT_BYTES) return { text, path: null, bytes };
  const dir = join(cwd, '.tunneldock', 'output_cache');
  mkdirSync(dir, { recursive: true });
  const id = createHash('sha256').update(text).digest('hex');
  const path = join(dir, `${id}.txt`);
  if (!existsSync(path)) {
    try { writeFileSync(path, text, { encoding: 'utf8', flag: 'wx' }); }
    catch (error) {
      // Parallel tools may have published the same content-addressed result.
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
    }
  }
  const header = `[TunnelDock: ${bytes} UTF-8 bytes saved intact to ${path}. Output below is PARTIAL. Use read with narrow line ranges; do not infer full success or absence of errors from this excerpt.]\n`;
  const tail = Array.from(text).slice(-600).join('');
  const separator = '\n\n[… middle omitted; full result retained …]\n\n';
  const findings = text.split('\n').map((line, index) => ({line,index}))
    .filter(({line}) => /\b(error|failed|failure|panic|exception|blocked)\b|错误|失败|阻塞/i.test(line))
    .slice(0, 6).map(({line,index}) => `L${index+1}: ${utf8Head(line,200)}`).join('\n');
  const diagnostic = findings ? `\nSelected diagnostic lines (not exhaustive; inspect full source):\n${findings}\n` : '';
  const budget = MAX_TEXT_BYTES - Buffer.byteLength(header + diagnostic + separator + tail);
  return { text: header + diagnostic + utf8Head(text, Math.max(0, budget)) + separator + tail, path, bytes };
}
function scoped(cwd: string): boolean {
  try {
    const binding = JSON.parse(readFileSync(join(cwd, '.tunneldock', 'session_binding.json'), 'utf8'));
    return typeof binding.project_id === 'string' && typeof binding.cwd === 'string'
      && resolve(binding.cwd).toLowerCase() === resolve(cwd).toLowerCase();
  } catch { return false; }
}
export default function (pi: ExtensionAPI) {
  pi.on('tool_result', (event, ctx) => {
    if (!scoped(ctx.cwd)) return;
    const text = event.content.filter(c => c.type === 'text').map(c => c.text).join('\n');
    try {
      const encoded = JSON.stringify(event.details);
      const largeDetails = Boolean(encoded && Buffer.byteLength(encoded) > MAX_TEXT_BYTES);
      if (Buffer.byteLength(text) <= MAX_TEXT_BYTES && !largeDetails) return;
      const bounded = boundText(ctx.cwd, text);
      // Metadata may be large even when the human-readable text is tiny.
      const nonText = event.content.filter(c => c.type !== 'text');
      let details = event.details;
      if (largeDetails && encoded) {
        const saved = boundText(ctx.cwd, encoded);
        details = { fullDetailsPath: saved.path, originalTool: event.toolName, truncated: true };
      }
      return { content: [{type: 'text' as const, text: bounded.text}, ...nonText], details, isError: event.isError };
    } catch (error) {
      // Never silently discard an output when the evidence store cannot be written.
      return { content: [{type: 'text' as const, text: utf8Head(`[TunnelDock output storage failed: ${String(error)}. The tool has already executed; do not blindly rerun a write. Inspect its artifact before continuing.]`, MAX_TEXT_BYTES)}, ...event.content.filter(c => c.type !== 'text')],
        details: { originalTool: event.toolName, outputStorageFailed: true }, isError: true };
    }
  });
}
