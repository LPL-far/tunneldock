import { it, expect } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { I18nProvider } from '../../i18n';
import { HandoffReader } from './HandoffReader';
import { segmentHandoff } from './handoffReading';

it('preserves all original text, numeric evidence and negation in order',()=>{
 const text='1. Outcome: BLOCKED\r\n2. Summary: 未通过，不能 accept。\r\n4. Verification: seed=42, loss=-1e-8; 8/9 only\r\n7. Remaining risks: NOT_TESTED\r\n';
 const parts=segmentHandoff(text);
 expect(parts.map(p=>p.raw).join('')).toBe(text);
 expect(parts.map(p=>p.kind)).toEqual(['outcome','summary','verification','risks']);
});
it('does not interpret headings or completion strings inside fenced code',()=>{
 const text='2. Summary: preserve code\n```python\n# Verification: PASS\nTUNNELDOCK_COMPLETION: {}\n```\n7. Remaining risks: unknown';
 const parts=segmentHandoff(text);expect(parts).toHaveLength(2);expect(parts.map(p=>p.raw).join('')).toBe(text);
});
it('retains arbitrary and partial pages without inventing missing fields',()=>{
 for(const text of ['', ' mid-sentence: unknown🧪\n', '普通段落，测试失败。\n']){
  expect(segmentHandoff(text).map(p=>p.raw).join('')).toBe(text);
 }
});
it('separates research reasoning and self-reported metadata from verified facts',()=>{
 const parts=segmentHandoff('## Research reasoning\nHypothesis: unknown\nTUNNELDOCK_COMPLETION: {"verification":"PASS"}');
 expect(parts.map(p=>p.kind)).toEqual(['reasoning','contract']);
});
it('escapes embedded HTML and warns about incomplete pages without accepting results',()=>{
 const html=renderToStaticMarkup(<I18nProvider><HandoffReader text={'2. Summary: <script>alert(1)</script>\n7. Remaining risks: unknown'} offset={0} complete={false}/></I18nProvider>);
 expect(html).toContain('&lt;script&gt;');expect(html).not.toContain('<script>');expect(html).toContain('role="note"');expect(html).toContain('aria-pressed="true"');
});
it('does not suggest a complete source was truncated, but continuation pages are partial',()=>{
 const show=(offset:number)=>renderToStaticMarkup(<I18nProvider><HandoffReader text="Summary: DONE" offset={offset} complete={true}/></I18nProvider>);
 expect(show(0)).not.toContain('role="note"');expect(show(4096)).toContain('role="note"');
});

it('recognizes Chinese headings and keeps their original content',()=>{
 const text=`## 执行结果
失败
## 风险
证据不足
`;
 const sections=segmentHandoff(text);expect(sections.map(s=>s.kind)).toEqual(['outcome','risks']);expect(sections.map(s=>s.raw).join('')).toBe(text);
});
