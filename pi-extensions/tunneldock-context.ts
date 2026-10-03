// Thin adapter to the local derived-context engine. Never submits tasks or accepts evidence.
import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { execFile } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
interface Input { action:'status'|'search'|'read'|'impact'; query?:string; kind?:'all'|'memory'|'code'|'artifact'; sha256?:string; offset?:number; path?:string }
export function engineArgs(cwd:string,p:Input):string[] {
  if(p.action==='status') return ['status',cwd];
  if(p.action==='search') {
    if(!p.query?.trim() || p.query.length>300) throw Error('Query requires 1–300 characters');
    return ['search',cwd,p.query,p.kind||'all'];
  }
  if(p.action==='read') {
    if(!/^[a-f0-9]{64}$/.test(p.sha256||'') || !Number.isInteger(p.offset??0) || (p.offset??0)<0) throw Error('Invalid evidence digest/range');
    return ['read',cwd,p.sha256!,String(p.offset||0),'4096',p.kind==='artifact'?'artifact':'memory'];
  }
  if(p.action==='impact') {if(!p.path)throw Error('Indexed relative file path required');return ['impact',cwd,p.path];}
  throw Error('Unknown context action');
}
export default function (pi:ExtensionAPI) {
  pi.registerTool({name:'tunneldock_context',label:'Search versioned project context',
    description:'Search local canonical-memory snapshots, source symbols or cached tool output BEFORE large reads or new code. Read exact immutable evidence by SHA256 with byte pagination; inspect lexical dependency/test candidates. Automatic working-set compression never accepts scientific conclusions. Impact candidates are NOT a semantic call graph or proof that code can be deleted.',
    parameters:{type:'object',required:['action'],additionalProperties:false,properties:{action:{type:'string',enum:['status','search','read','impact']},query:{type:'string',maxLength:300},kind:{type:'string',enum:['all','memory','code','artifact']},sha256:{type:'string'},offset:{type:'integer',minimum:0},path:{type:'string'}}} as any,
    async execute(_id:string,p:Input,signal:AbortSignal|undefined,_update:unknown,ctx:{cwd:string}) {
      const binding=JSON.parse(readFileSync(join(ctx.cwd,'.tunneldock/session_binding.json'),'utf8'));
      if(!binding.project_id || resolve(binding.cwd).toLowerCase()!==resolve(ctx.cwd).toLowerCase()) throw Error('Project identity mismatch');
      const script=join(process.env.APPDATA||join(homedir(),'.local','share'),'TunnelDock/runtime/context-engine.cjs');
      const text=await new Promise<string>((yes,no)=>execFile(process.execPath,[script,...engineArgs(ctx.cwd,p)],{windowsHide:true,timeout:25000,maxBuffer:32768,signal},(err,stdout,stderr)=>err?no(Error(String(stderr||err))):yes(stdout)));
      return {content:[{type:'text' as const,text}],details:{project_id:binding.project_id,action:p.action,canonical_modified:false}};
    }
  });
}
