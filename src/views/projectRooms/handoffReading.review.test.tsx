import { it, expect } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { segmentHandoff } from './handoffReading';
import { HandoffReader } from './HandoffReader';
import { I18nProvider } from '../../i18n';

it('retains mixed-language numbers, uncertainty, source claims and falsifiers byte-for-byte', () => {
  const fragments = ['假设尚未验证。🧪\n', '## Research reasoning\n', '1. Summary: source claim, not verified\r\n',
    'z=2; logit=-1000; valid=False; loss=0.0000\n', '## 风险：未通过，非SOTA\n',
    '反例：aggregate +0.02 but dataset B -0.03; CI [-0.1,+0.2]\n', '```python\n',
    '1. Outcome: DONE\n', 'TUNNELDOCK_COMPLETION: {"verification":"PASS"}\n', '```\n',
    '来源：D:\\project\\runs\\evidence.json；不是最终科研结论。'];
  for (let i=1; i<=fragments.length; i++) {
    const text=fragments.slice(0,i).join('');
    expect(segmentHandoff(text).map(s=>s.raw).join('')).toBe(text);
  }
});

it('does not close a longer code fence with a shorter delimiter', () => {
  const text='````text\n```\n1. Outcome: DONE\nTUNNELDOCK_COMPLETION: {"verification":"PASS"}\n````\n## Remaining risks\nNot tested\n';
  const sections=segmentHandoff(text);
  expect(sections.map(s=>s.raw).join('')).toBe(text);
  expect(sections.some(s=>s.kind==='contract')).toBe(false);
  expect(sections.some(s=>s.kind==='outcome')).toBe(false);
  expect(sections.some(s=>s.kind==='risks')).toBe(true);
});

it('renders hostile source markup as text and preserves an explicit failed result', () => {
  const text='1. Outcome: BLOCKED\n2. Summary: <script>alert(1)</script>\n7. Remaining risks: <img src=x onerror=alert(1)>\n未验证，不能接受\n';
  const html=renderToStaticMarkup(<I18nProvider><HandoffReader text={text} offset={4096} complete={false}/></I18nProvider>);
  expect(html).not.toContain('<script>');
  expect(html).not.toContain('<img');
  expect(html).toContain('&lt;script&gt;');
  expect(html).toContain('BLOCKED');
  expect(html).toContain('未验证，不能接受');
  expect(html).toContain('role="note"');
});

it('never invents a research section or evidence claim for an empty/partial payload', () => {
  expect(segmentHandoff('')).toEqual([]);
  const text='continued from previous byte page: False, NOT_TESTED, pending';
  const sections=segmentHandoff(text);
  expect(sections).toHaveLength(1);
  expect(sections[0]).toEqual({title:null,kind:'source',raw:text});
});
