'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {compact}=require('../runtime/memory-compaction.cjs');
const engine=require('../runtime/context-engine.cjs');
test('automatic canonical pagination keeps cold counterevidence searchable',t=>{
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'td-memory-context-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 fs.mkdirSync(path.join(root,'.project_memory'));fs.mkdirSync(path.join(root,'.tunneldock'));
 fs.writeFileSync(path.join(root,'.tunneldock/session_binding.json'),JSON.stringify({project_id:'fixture',cwd:root}));
 const original=Buffer.from('# Current evidence\n'+('Established constraint; do not change the evaluation split.\n'.repeat(600))+'\n# Counterevidence\nUNIQUE_NEGATIVE_TRANSFER: NOT accepted, metric -0.23.\n');
 fs.writeFileSync(path.join(root,'.project_memory/PROJECT_STATE.md'),original);
 const result=compact(root,{apply:true,quietMs:0});assert.equal(result.state,'compacted');
 assert.ok(!fs.readFileSync(path.join(root,'.project_memory/PROJECT_STATE.md'),'utf8').includes('UNIQUE_NEGATIVE_TRANSFER'));
 const status=engine.refresh(root,'fixture');assert.equal(status.phase,'ready');
 const found=engine.search(root,'UNIQUE_NEGATIVE_TRANSFER','memory');assert.ok(found.hits.some(h=>h.path.includes('/archive/auto/')&&h.text.includes('NOT accepted')));
 const row=result.changed_files[0];assert.deepEqual(fs.readFileSync(path.join(root,'.project_memory',row.archive)),original);
 const pointer=fs.readFileSync(path.join(root,'.tunneldock/context/current.json'));
 fs.writeFileSync(path.join(root,'.project_memory',row.archive),'corrupt');assert.throws(()=>engine.refresh(root,'fixture'));
 assert.deepEqual(fs.readFileSync(path.join(root,'.tunneldock/context/current.json')),pointer);
});
