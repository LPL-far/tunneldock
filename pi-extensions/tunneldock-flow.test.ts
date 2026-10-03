import { it, expect } from 'vitest';
import { mkdtempSync, rmSync, readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import extension, { boundText, utf8Head, MAX_TEXT_BYTES } from './tunneldock-flow';
it('caps UTF-8 bytes and preserves complete content-addressed evidence', () => {
 const dir=mkdtempSync(join(tmpdir(),'td-flow-'));
 try { const text='证据🧪'.repeat(6000); const result=boundText(dir,text);
 expect(Buffer.byteLength(result.text)).toBeLessThanOrEqual(MAX_TEXT_BYTES);
 expect(result.text).not.toContain('\uFFFD'); expect(readFileSync(result.path!,'utf8')).toBe(text);
 expect(boundText(dir,text).path).toBe(result.path);
 } finally {rmSync(dir,{recursive:true,force:true});}
});
it('does not alter small results',()=>{ expect(boundText('.', 'PASS').text).toBe('PASS'); expect(utf8Head('证据',4)).toBe('证'); });
it('retains the error flag and non-text content through the real hook',()=>{
 const dir=mkdtempSync(join(tmpdir(),'td-flow-hook-')); let handler: any;
 try {mkdirSync(join(dir,'.tunneldock'));writeFileSync(join(dir,'.tunneldock/session_binding.json'),JSON.stringify({project_id:'fixture',cwd:dir}));
 extension({on:(_name:string,h:any)=>{handler=h;}} as any);
 const image={type:'image',data:'fixture',mimeType:'image/png'};
 const result=handler({content:[{type:'text',text:'x'.repeat(50000)},image],toolName:'bash',isError:true,details:{}},{cwd:dir});
 expect(result.isError).toBe(true);expect(result.content[1]).toBe(image);
 expect(Buffer.byteLength(result.content[0].text)).toBeLessThanOrEqual(MAX_TEXT_BYTES);
 } finally {rmSync(dir,{recursive:true,force:true});}
});
it('does not touch unrelated projects',()=>{
 let h:any; extension({on:(_n:string,handler:any)=>{h=handler;}} as any);
 expect(h({content:[{type:'text',text:'x'.repeat(50000)}]}, {cwd:tmpdir()})).toBeUndefined();
});
