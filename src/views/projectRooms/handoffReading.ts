export interface HandoffSection { title: string | null; kind: string; raw: string }

const headings: Array<[RegExp, string]> = [
  [/^(outcome|结果|执行结果)(?:\b|[：:]|$)/i, 'outcome'],
  [/^(summary|摘要|总结|结论)(?:\b|[：:]|$)/i, 'summary'],
  [/^(files changed|修改文件)(?:\b|[：:]|$)/i, 'files'],
  [/^(verification|tests?(?:\s*\/\s*experiment evidence)?|验证|测试)(?:\b|[：:]|$)/i, 'verification'],
  [/^(evidence(?:\s*\/\s*artifacts)?|证据|产物)(?:\b|[：:]|$)/i, 'evidence'],
  [/^(cleanup|清理)(?:\b|[：:]|$)/i, 'cleanup'],
  [/^(remaining risks?(?:\s*\/\s*blockers)?|risks?|剩余风险|风险|阻塞)(?:\b|[：:]|$)/i, 'risks'],
  [/^(requested review|questions(?: or requested review)?|待审核|审核请求)(?:\b|[：:]|$)/i, 'review'],
  [/^(research reasoning|科研论证)(?:\b|[：:]|$)/i, 'reasoning'],
  [/^(memory(?:\s*\/\s*experiment)? updates?|记忆更新)(?:\b|[：:]|$)/i, 'memory'],
];

// Display-only segmentation. Preserve EVERY character, order and qualifier.
// A byte page may start/end midway through a section; never infer absent evidence.
export function segmentHandoff(text: string): HandoffSection[] {
  const sections: HandoffSection[] = [];
  let fence: { char: string; width: number } | null = null;
  for (const line of text.match(/[^\n]*\n|[^\n]+$/g) ?? []) {
    const clean = line.trim();
    const marker = clean.match(/^(`{3,}|~{3,})/);
    let title: string | null = null;
    let kind = 'source';
    if (marker) {
      const char = marker[1][0];
      if (!fence) fence = { char, width: marker[1].length };
      else if (char === fence.char && marker[1].length >= fence.width && clean === marker[1]) fence = null;
    } else if (!fence) {
      const candidate = clean.replace(/^#{1,6}\s+/, '').replace(/^\d+[.)]\s+/, '').replace(/^\*\*/, '').replace(/\*\*/g, '');
      const found = headings.find(([pattern]) => pattern.test(candidate));
      // Only numbered/Markdown headings, or a known label with a colon.
      if (found && (/^(?:#{1,6}\s|\d+[.)]\s|\*\*)/.test(clean) || /^[^:：]{1,65}[:：]/.test(candidate))) {
        title = candidate.split(/[:：]/, 1)[0]; kind = found[1];
      } else if (clean.startsWith('TUNNELDOCK_COMPLETION:')) {
        title = 'TUNNELDOCK_COMPLETION'; kind = 'contract';
      }
    }
    if (title !== null || sections.length === 0) sections.push({ title, kind, raw: line });
    else sections[sections.length - 1].raw += line;
  }
  return sections;
}
