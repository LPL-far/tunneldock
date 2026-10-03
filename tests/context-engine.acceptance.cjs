const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),crypto=require('node:crypto');
const {refresh,search,searchArtifacts,readObject,impact,safe,utf8Head,LIMITS}=require('../runtime/context-engine.cjs');
function fixture(fn){const dir=fs.mkdtempSync(path.join(os.tmpdir(),'td-context-'));try{return fn(dir);}finally{fs.rmSync(dir,{recursive:true,force:true});}}
function put(root,name,text){fs.mkdirSync(path.dirname(path.join(root,name)),{recursive:true});fs.writeFileSync(path.join(root,name),text);}
const digest=b=>crypto.createHash('sha256').update(b).digest('hex');
test('automatic projection stays bounded and canonical memory stays byte-identical',()=>fixture(root=>{
 const text='# Status\nNEVER accept an unreviewed result.\n'+'## Case\nNot accepted — hypothesis remains pending.\n'.repeat(2000);
 put(root,'.project_memory/PROJECT_STATE.md',text);const before=digest(Buffer.from(text));
 const status=refresh(root,'fixture');assert.equal(status.phase,'ready');assert.equal(status.canonical_modified,false);
 assert(status.hot_bytes<=LIMITS.hot);assert.equal(digest(fs.readFileSync(path.join(root,'.project_memory/PROJECT_STATE.md'))),before);
 const hot=fs.readFileSync(path.join(root,status.hot_path),'utf8');assert.match(hot,/NEVER accept/);assert.match(hot,/NOT canonical/);
}));
test('unchanged content does not produce another generation',()=>fixture(root=>{
 put(root,'.project_memory/PROJECT_STATE.md','# Stable');refresh(root);const n=fs.readdirSync(path.join(root,'.tunneldock/context/generations')).length;
 assert.equal(refresh(root).changed,false);assert.equal(fs.readdirSync(path.join(root,'.tunneldock/context/generations')).length,n);
}));
test('search cites exact version and reports live source drift',()=>fixture(root=>{
 put(root,'.project_memory/RESULTS.md','# Results\nResult rejected: finite-gradient gate failed.');refresh(root);
 const h=search(root,'rejected').hits[0];assert.equal(h.line,2);assert.equal(h.current,true);
 put(root,'.project_memory/RESULTS.md','# Results\nChanged concurrently');assert.equal(search(root,'rejected').hits[0].current,false);
 assert.match(readObject(root,h.sha256).text,/gate failed/);
}));
test('UTF-8 paging is byte-correct and evidence hash is checked',()=>fixture(root=>{
 put(root,'.project_memory/RESULTS.md','证据🧪'.repeat(2000));refresh(root);const h=search(root,'证据').hits[0];
 let o=0,t='';for(;;){const p=readObject(root,h.sha256,o);assert(!p.text.includes('\uFFFD'));t+=p.text;if(p.complete)break;o=p.next_offset;}
 assert.equal(t,'证据🧪'.repeat(2000));assert.throws(()=>readObject(root,h.sha256,1),/UTF-8/);
 put(root,'.tunneldock/context/objects/'+h.sha256+'.txt','changed');assert.throws(()=>readObject(root,h.sha256),/checksum/);
}));
test('lexical impact is conservative and never a deletion approval',()=>fixture(root=>{
 put(root,'src/helper.ts','export function helper() {}');put(root,'src/use.ts',"import {helper} from './helper';\nhelper();");
 put(root,'tests/helper.test.ts',"import {helper} from '../src/helper';");refresh(root);
 const v=impact(root,'src/helper.ts');assert.equal(v.can_prove_unused,false);assert.equal(v.complete,false);assert(v.candidate_dependents.includes('src/use.ts'));assert(v.candidate_tests.includes('tests/helper.test.ts'));
}));
test('live refresh lease is never stolen',()=>fixture(root=>{
 put(root,'.tunneldock/context/refresh.lock',JSON.stringify({pid:process.pid}));assert.equal(refresh(root).phase,'busy');
 assert(fs.existsSync(path.join(root,'.tunneldock/context/refresh.lock')));
}));
test('path escapes and symlinks are rejected',()=>fixture(root=>{
 assert.throws(()=>safe(root,'../out'));assert.throws(()=>safe(root,path.resolve(root,'other')));
 const other=fs.mkdtempSync(path.join(os.tmpdir(),'td-out-'));
 try {fs.symlinkSync(other,path.join(root,'src'),process.platform==='win32'?'junction':'dir');assert.throws(()=>refresh(root),/Symlink/);}
 finally {fs.rmSync(other,{recursive:true,force:true});}
}));
test('oversized source is disclosed, not silently called a complete index',()=>fixture(root=>{
 put(root,'src/huge.ts','x'.repeat(LIMITS.codeFile+1));const r=refresh(root);assert.equal(r.omitted_count,1);
}));
test('malformed pointer and invalid hashes fail closed',()=>fixture(root=>{
 refresh(root);assert.throws(()=>readObject(root,'../../secret'));
 put(root,'.tunneldock/context/current.json',JSON.stringify({index:'../secret',revision:'fake'}));assert.throws(()=>search(root,'anything'));
}));
test('artifact search preserves complete cached evidence and flags partial search',()=>fixture(root=>{
 const text='Header\nERROR decisive original error\n'+'x'.repeat(20000);const id=digest(text);
 put(root,'.tunneldock/output_cache/'+id+'.txt',text);const v=searchArtifacts(root,'ERROR');assert.equal(v.hits[0].line,2);assert.equal(v.complete,false);
 assert.match(readObject(root,id,0,6144,'artifact').text,/decisive original/);
}));
test('long canonical file is rejected without overwriting last good generation',()=>fixture(root=>{
 put(root,'.project_memory/RESULTS.md','old result');refresh(root);const previous=fs.readFileSync(path.join(root,'.tunneldock/context/current.json'),'utf8');
 put(root,'.project_memory/RESULTS.md','x'.repeat(LIMITS.memoryFile+1));assert.throws(()=>refresh(root),/budget/);assert.equal(fs.readFileSync(path.join(root,'.tunneldock/context/current.json'),'utf8'),previous);
}));
test('small unicode helper never splits a code point',()=>{assert.equal(utf8Head('证据',4),'证');});
