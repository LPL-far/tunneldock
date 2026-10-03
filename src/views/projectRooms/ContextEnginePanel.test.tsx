import { renderToStaticMarkup } from 'react-dom/server';
import { describe, it, expect } from 'vitest';
import { ContextEnginePanel } from './ContextEnginePanel';
import { I18nProvider } from '../../i18n';
import type { ContextEngineState } from '../../types/activity';
const checked='2026-10-03T06:00:00Z';
const ready:ContextEngineState={phase:'ready',checked_at:checked,memory_source_bytes:65536,hot_bytes:12000,indexed_code_files:32,omitted_count:2,canonical_modified:false};
const render=(state?:ContextEngineState,now=Date.parse(checked))=>renderToStaticMarkup(<I18nProvider><ContextEnginePanel state={state} now={now} cwd="D:/fixture" projectId="fixture"/></I18nProvider>);
describe('context status is observed rather than claimed',()=>{
 it('shows source and working-set sizes with explicit evidence boundary',()=>{
  const html=render(ready);expect(html).toContain('64.0 KiB');expect(html).toContain('11.7 KiB');expect(html).toContain('32');expect(html).toMatch(/不完整|incomplete/);expect(html).not.toContain('100%');
 });
 it('distinguishes stale and blocked state from a successful refresh',()=>{
  expect(render(ready,Date.parse(checked)+200000)).toMatch(/较旧|stale/);
  expect(render({...ready,phase:'blocked',error:'concurrent change'})).toContain('concurrent change');
 });
 it('does not invent completed compression when state is absent',()=>{
  expect(render()).toMatch(/首次索引|first index/);expect(render()).not.toContain('11.7 KiB');
 });
});
