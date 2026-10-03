import { renderToStaticMarkup } from 'react-dom/server';
import { it, expect } from 'vitest';
import { ProjectRoomHeader } from './ProjectRoomHeader';
import { I18nProvider } from '../../i18n';
import type { ProjectRoomSnapshot } from '../../types';
it('keeps project identity visible while capacity details are collapsed',()=>{
 const room={config:{name:'Research room',id:'fixture',local_root:'D:/fixture',remote:{root:''}},agents:[]} as unknown as ProjectRoomSnapshot;
 const html=renderToStaticMarkup(<I18nProvider><ProjectRoomHeader room={room} selectedSummary={null} busy={false} capacities={new Map()} runtimeMap={new Map()} projectPrompt={null} promptCopied={false} onOpenProjectPrompt={()=>{}} onCopyProjectPrompt={()=>{}}/></I18nProvider>);
 expect(html).toContain('Research room');expect(html).toContain('<details');expect(html).toContain('<summary');expect(html).not.toMatch(/<details[^>]*\sopen/);
});
