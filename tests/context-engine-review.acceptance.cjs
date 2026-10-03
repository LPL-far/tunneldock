'use strict';
// Independent integrity/regression checks for the derived-context engine.
// Fixtures are isolated: this file never accesses real project/research state.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),crypto=require('node:crypto');
const engine=require('../runtime/context-engine.cjs');
function fixture(fn){const root=fs.mkdtempSync(path.join(os.tmpdir(),'td-context-review-'));try{return fn(root);}finally{fs.rmSync(root,{recursive:true,force:true});}}
function put(root,name,body){const p=path.join(root,name);fs.mkdirSync(path.dirname(p),{recursive:true});fs.writeFileSync(p,body);return p;}
function get(root,name){return JSON.parse(fs.readFileSync(path.join(root,name),'utf8'));}
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const statePath='.tunneldock/context/status.json';
const currentPath='.tunneldock/context/current.json';

test('invalid status must not strand a live-owner lease that blocks every refresh',()=>fixture(root=>{
 put(root,'.project_memory/PROJECT_STATE.md','# State\nCurrent source.');
 engine.refresh(root,'fixture');put(root,statePath,'{invalid json');
 try{engine.refresh(root,'fixture');}catch{} // an explicit error is permitted
 assert.equal(fs.existsSync(path.join(root,'.tunneldock/context/refresh.lock')),false,'failed refresh left a live-owner lease');
}));

test('source-object corruption is rejected by search, not only full read',()=>fixture(root=>{
 put(root,'.project_memory/RESULTS.md','# Results\nNo accepted result yet.');
 engine.refresh(root,'fixture');const hit=engine.search(root,'accepted').hits[0];
 put(root,'.tunneldock/context/objects/'+hit.sha256+'.txt','# Results\nAccepted result fabricated.');
 assert.throws(()=>engine.search(root,'fabricated'),/checksum|corrupt|digest|integrity/i,'corrupt bytes were returned with a trusted source digest');
}));

test('newly omitted source changes coverage even if indexed source hashes are unchanged',()=>fixture(root=>{
 put(root,'src/helper.ts','export function helper() {}');engine.refresh(root,'fixture');
 put(root,'src/oversized.ts','x'.repeat(engine.LIMITS.codeFile+1));
 const next=engine.refresh(root,'fixture'),pointer=get(root,currentPath),index=get(root,pointer.index);
 assert(next.omitted_count>=1,'maintenance status falsely reports no omitted sources');
 assert(index.omitted.some(x=>x.path==='src/oversized.ts'),'new excluded file vanished from coverage');
}));

test('impact query discloses source changes since its index snapshot',()=>fixture(root=>{
 put(root,'src/helper.ts','export function originalHelper() {}');engine.refresh(root,'fixture');
 put(root,'src/helper.ts','export function differentHelper() {}');
 const view=engine.impact(root,'src/helper.ts');
 assert(view.current===false||view.stale===true||view.changed_since_index===true,'outdated impact data was not marked stale');
 assert.equal(view.can_prove_unused,false);
}));

test('symlinked source objects are rejected during immutable reuse',()=>fixture(root=>{
 const body='# Canonical\nNo outside references.';put(root,'.project_memory/RESULTS.md',body);
 const objects=path.join(root,'.tunneldock/context/objects');fs.mkdirSync(objects,{recursive:true});
 const target=put(root,'outside-cache.txt',body),dest=path.join(objects,hash(Buffer.from(body))+'.txt');
 try{fs.symlinkSync(target,dest,'file');}catch(e){if(e.code==='EPERM')return;throw e;}
 assert.throws(()=>engine.refresh(root,'fixture'),/symlink|reparse|symbolic/i,'immutable object followed an unapproved symlink');
}));

test('binary canonical memory fails closed without publishing a partial replacement',()=>fixture(root=>{
 put(root,'.project_memory/RESULTS.md','# Accepted source\nNot a conclusion.');engine.refresh(root,'fixture');
 const before=fs.readFileSync(path.join(root,currentPath),'utf8');
 put(root,'.project_memory/RESULTS.md',Buffer.from([0x52,0,0xff,0x80]));
 assert.throws(()=>engine.refresh(root,'fixture'),/binary|UTF.?8|canonical|encoding/i);
 assert.equal(fs.readFileSync(path.join(root,currentPath),'utf8'),before,'invalid canonical bytes replaced the last usable snapshot');
}));
