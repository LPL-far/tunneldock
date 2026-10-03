'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const {compact, expand, BUDGETS, LEASE, main} = require('../runtime/memory-compaction.cjs');
const digest = data => crypto.createHash('sha256').update(data).digest('hex');
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'td-compaction-'));
  t.after(() => fs.rmSync(root, {recursive:true, force:true}));
  fs.mkdirSync(path.join(root,'.tunneldock'));
  fs.mkdirSync(path.join(root,'.project_memory'));
  fs.writeFileSync(path.join(root,'.tunneldock/session_binding.json'), JSON.stringify({version:1, project_id:'fixture', cwd:root}));
  return root;
}
const source = root => path.join(root,'.project_memory/PROJECT_STATE.md');
const large = () => Buffer.from('# Evidence\r\n' + '不允许删除 🧪; NOT 0.25 mg; −2.3e-7 m/s\r\n'.repeat(600));
const apply = root => compact(root,{apply:true,quietMs:0});

test('UTF-8 and complete lines; exact archive retrieval; stable no-change refresh', t => {
  const root=fixture(t), original=large(); fs.writeFileSync(source(root),original);
  const dry=compact(root,{quietMs:0}); assert.equal(dry.state,'dry_run');
  assert.deepEqual(fs.readFileSync(source(root)),original);
  assert.equal(fs.existsSync(path.join(root,'.project_memory/archive')),false);
  const result=apply(root); assert.equal(result.state,'compacted'); assert.equal(result.changed_files.length,1);
  const row=result.changed_files[0], current=fs.readFileSync(source(root));
  assert.ok(current.length<=BUDGETS['PROJECT_STATE.md']);
  assert.ok(Buffer.from(current.toString('utf8')).equals(current));
  assert.deepEqual(fs.readFileSync(path.join(root,'.project_memory',row.archive)),original);
  assert.equal(row.sha256,digest(original));
  assert.ok(current.subarray(row.notice_bytes).equals(original.subarray(0,row.retained_bytes)));
  assert.equal(original[row.retained_bytes-1],10);
  assert.match(current.toString(),/Unseen content still applies/);
  assert.match(current.toString(),/Full expansion is mandatory/);
  assert.ok(expand(root,'PROJECT_STATE.md').some(doc=>doc.data.equals(original)));
  apply(root);
  const status=path.join(root,'.project_memory/AUTO_COMPACTION_STATUS.json');
  const prior=fs.statSync(status).mtimeMs; const bytes=fs.readFileSync(status);
  assert.equal(apply(root).state,'unchanged');
  assert.equal(fs.statSync(status).mtimeMs,prior); assert.deepEqual(fs.readFileSync(status),bytes);
});

test('all nine exact byte thresholds and no unrelated file changes', t => {
  const root=fixture(t);
  for(const [name,budget] of Object.entries(BUDGETS)) fs.writeFileSync(path.join(root,'.project_memory',name),Buffer.alloc(budget,10));
  fs.writeFileSync(path.join(root,'.project_memory/UNRELATED.md'),large());
  assert.equal(apply(root).state,'unchanged');
  for(const name of Object.keys(BUDGETS)) fs.appendFileSync(path.join(root,'.project_memory',name),'\n');
  const result=apply(root); assert.equal(result.changed_files.length,9);
  for(const row of result.changed_files) {assert.equal(row.before_bytes,BUDGETS[row.file]+1);assert.ok(row.after_bytes<=BUDGETS[row.file]);}
  assert.deepEqual(fs.readFileSync(path.join(root,'.project_memory/UNRELATED.md')),large());
});

test('overlong first line and invalid UTF-8 block without replacement', t => {
  const root=fixture(t);
  for(const data of [Buffer.alloc(17000,97),Buffer.concat([large(),Buffer.from([255])])]) {
    fs.writeFileSync(source(root),data); assert.equal(apply(root).state,'blocked'); assert.deepEqual(fs.readFileSync(source(root)),data);
  }
});

test('policy opt-in, explicit dry run and policy-only caller', t => {
  const root=fixture(t); fs.writeFileSync(source(root),large());
  assert.equal(compact(root,{apply:true,policyEnabledOnly:true,quietMs:0}).state,'disabled');
  fs.writeFileSync(path.join(root,'.tunneldock/memory-policy.json'),'{"auto_compact":true}');
  assert.equal(compact(root,{apply:false,quietMs:0}).state,'dry_run');
  assert.equal(compact(root,{policyEnabledOnly:true,quietMs:0}).state,'compacted');
});

test('recent writer and existing leases are busy, never stolen', t => {
  const root=fixture(t); fs.writeFileSync(source(root),large());
  assert.equal(compact(root,{apply:true,quietMs:60000}).state,'busy');
  const lock=path.join(root,LEASE);
  for(const value of [JSON.stringify({pid:process.pid}),'unverifiable owner',JSON.stringify({pid:99999999})]) {
    fs.writeFileSync(lock,value); assert.equal(apply(root).state,'busy'); assert.equal(fs.readFileSync(lock,'utf8'),value);
  }
});

test('missing or mismatched identity never writes memory', t => {
  const root=fixture(t); fs.writeFileSync(source(root),large()); const binding=path.join(root,'.tunneldock/session_binding.json');
  for(const value of [{cwd:root},{project_id:'fixture',cwd:path.dirname(root)}]) {
    fs.writeFileSync(binding,JSON.stringify(value)); assert.equal(apply(root).state,'blocked');
  }
  fs.unlinkSync(binding); assert.equal(apply(root).state,'blocked'); assert.deepEqual(fs.readFileSync(source(root)),large());
});

test('source hash checked after staging; writer content survives', t => {
  const root=fixture(t); fs.writeFileSync(source(root),large());
  const link=fs.linkSync; let fired=false;
  fs.linkSync=function(from,to) {
    if(!fired && String(to).endsWith('.md.json')) {fired=true;fs.appendFileSync(source(root),'CONCURRENT writer\n');}
    return link.call(fs,from,to);
  };
  let result; try {result=apply(root);} finally {fs.linkSync=link;}
  assert.ok(fired); assert.equal(result.state,'busy');
  assert.deepEqual(fs.readFileSync(source(root)),Buffer.concat([large(),Buffer.from('CONCURRENT writer\n')]));
  assert.equal(apply(root).state,'compacted');
});

test('replacement failure preserves source and retries verified staged objects', t => {
  const root=fixture(t); fs.writeFileSync(source(root),large()); const rename=fs.renameSync;
  fs.renameSync=function(from,to) {if(to===source(root)) throw Object.assign(Error('injected replacement failure'),{code:'EACCES'});return rename.call(fs,from,to);};
  let result; try {result=apply(root);} finally {fs.renameSync=rename;}
  assert.equal(result.state,'blocked'); assert.deepEqual(fs.readFileSync(source(root)),large());
  assert.equal(apply(root).state,'compacted');
});

test('corrupt immutable archive is never overwritten', t => {
  const root=fixture(t), data=large();fs.writeFileSync(source(root),data);
  const archive=path.join(root,'.project_memory/archive/auto/PROJECT_STATE',digest(data)+'.md');
  fs.mkdirSync(path.dirname(archive),{recursive:true});fs.writeFileSync(archive,'corrupt');
  assert.equal(apply(root).state,'blocked');assert.deepEqual(fs.readFileSync(source(root)),data);assert.equal(fs.readFileSync(archive,'utf8'),'corrupt');
});

test('old chained references and new generations expand once per object', t => {
  const root=fixture(t), memory=path.join(root,'.project_memory');fs.mkdirSync(path.join(memory,'archive/old'),{recursive:true});
  const leaf=Buffer.from('NOT accepted: 0.003 kg\r\n');
  fs.writeFileSync(path.join(memory,'archive/old/leaf.md'),leaf);
  fs.writeFileSync(path.join(memory,'archive/old/page.md'),'# Prior page\n[leaf](leaf.md)\n[duplicate](leaf.md)\n');
  fs.writeFileSync(source(root),Buffer.concat([Buffer.from('[required](archive/old/page.md)\n'),large()]));
  assert.equal(apply(root).state,'compacted');
  for(let i=0;i<3;i++) {fs.appendFileSync(source(root),large());const result=apply(root);assert.equal(result.state,'compacted',JSON.stringify(result));}
  const docs=expand(root,'PROJECT_STATE.md');assert.equal(docs.filter(d=>d.data.equals(leaf)).length,1);
  assert.equal(docs.length,7);assert.equal(new Set(docs.map(d=>d.sha256)).size,docs.length);
});

test('symlink/junction directories and escaping archive references block', t => {
  const root=fixture(t), outside=fixture(t);fs.writeFileSync(source(root),large());
  const link=path.join(root,'.project_memory/archive');fs.symlinkSync(path.join(outside,'.project_memory'),link,'junction');
  assert.equal(apply(root).state,'blocked');assert.deepEqual(fs.readFileSync(source(root)),large());fs.unlinkSync(link);
  fs.writeFileSync(source(root),Buffer.concat([Buffer.from('[bad](archive/../../escape.md)\n'),large()]));
  assert.equal(apply(root).state,'blocked');
  const alias=path.join(root,'alias');fs.symlinkSync(outside,alias,'junction');assert.equal(apply(alias).state,'blocked');
});

test('CLI argument contract applies only explicit flag or policy', t => {
  const root=fixture(t);fs.writeFileSync(source(root),large());
  for(const [args,state] of [[[root],'dry_run'],[[root,'--apply'],'compacted']]) {
    const now=Date.now;Date.now=()=>now()+60000;
    try {assert.equal(main(args).state,state);} finally {Date.now=now;}
  }
  assert.throws(()=>main([]),/Usage/);assert.throws(()=>main([root,'--unknown']),/Usage/);
});

test('stable reads reject a concurrent edit during file read', t => {
  const root=fixture(t);fs.writeFileSync(source(root),large());const original=fs.readFileSync;
  let injected=false;
  fs.readFileSync=function(file,...args) {
    const data=original.call(fs,file,...args);
    if(!injected && Buffer.isBuffer(data) && data.equals(large())) {injected=true;fs.appendFileSync(source(root),'writer added 9 kg\n');}
    return data;
  };
  let result;try {result=apply(root);} finally {fs.readFileSync=original;}
  assert.ok(injected);assert.equal(result.state,'busy');assert.match(fs.readFileSync(source(root),'utf8'),/writer added 9 kg/);
});

test('immutable manifest corruption and missing continuation block expansion', t => {
  const root=fixture(t);fs.writeFileSync(source(root),large());const result=apply(root);
  const archive=path.join(root,'.project_memory',result.changed_files[0].archive), manifest=archive+'.json';
  const good=fs.readFileSync(manifest);fs.writeFileSync(manifest,'{}');
  assert.equal(apply(root).state,'blocked');assert.throws(()=>expand(root),/manifest/);
  fs.writeFileSync(manifest,good);fs.writeFileSync(archive,'corrupt');
  assert.equal(apply(root).state,'blocked');assert.throws(()=>expand(root),/checksum/);
  fs.unlinkSync(archive);assert.equal(apply(root).state,'blocked');
});

test('hardlinked canonical sources and status directories block before mutation', t => {
  const root=fixture(t);fs.writeFileSync(source(root),large());
  const alias=path.join(root,'hardlink.md');fs.linkSync(source(root),alias);
  assert.equal(apply(root).state,'blocked');fs.unlinkSync(alias);
  fs.unlinkSync(path.join(root,'.project_memory/AUTO_COMPACTION_STATUS.json'));
  fs.mkdirSync(path.join(root,'.project_memory/AUTO_COMPACTION_STATUS.json'));
  assert.equal(apply(root).state,'blocked');assert.deepEqual(fs.readFileSync(source(root)),large());
});

test('status failure leaves an expandable canonical page and refresh recovers', t => {
  const root=fixture(t);fs.writeFileSync(source(root),large());const rename=fs.renameSync;
  fs.renameSync=function(from,to) {if(to.endsWith('AUTO_COMPACTION_STATUS.json')) throw Error('injected status failure');return rename.call(fs,from,to);};
  let result;try {result=apply(root);} finally {fs.renameSync=rename;}
  assert.equal(result.state,'blocked');assert.equal(result.changed_files.length,1);assert.match(result.status_error,/injected/);
  assert.ok(expand(root).some(doc=>doc.data.equals(large())));
  assert.equal(apply(root).state,'unchanged');assert.ok(fs.existsSync(path.join(root,'.project_memory/AUTO_COMPACTION_STATUS.json')));
});

test('CR-only complete lines, resource ceilings, invalid policy, and missing memory', t => {
  const root=fixture(t);fs.writeFileSync(source(root),'# CR\r'+'NOT 1 µm\r'.repeat(2000));
  assert.equal(compact(root,{apply:true,quietMs:-1}).state,'blocked');
  assert.equal(compact(root,{apply:'yes'}).state,'blocked');
  assert.equal(compact(root,{maxObjects:0}).state,'blocked');
  assert.equal(compact(root,{maxFileBytes:128*1024*1024}).state,'blocked');
  assert.equal(compact(root,{apply:true,quietMs:0,maxTotalBytes:1}).state,'blocked');
  assert.equal(compact(root,{apply:true,quietMs:0,maxFileBytes:100}).state,'blocked');
  const result=apply(root);assert.equal(result.state,'compacted');
  const row=result.changed_files[0];assert.equal(fs.readFileSync(source(root))[row.notice_bytes+row.retained_bytes-1],13);
  fs.writeFileSync(path.join(root,'.tunneldock/memory-policy.json'),'null');assert.equal(apply(root).state,'blocked');
  const other=fixture(t);fs.rmdirSync(path.join(other,'.project_memory'));assert.equal(apply(other).state,'blocked');
});

test('legacy cycles use finite graph references; relative parent links stay in archive', t => {
  const root=fixture(t), memory=path.join(root,'.project_memory');fs.mkdirSync(path.join(memory,'archive/old/sub'),{recursive:true});
  fs.writeFileSync(path.join(memory,'archive/old/a.md'),'[b](sub/b.md)\n');
  fs.writeFileSync(path.join(memory,'archive/old/sub/b.md'),'[a](../a.md)\n');
  fs.writeFileSync(source(root),Buffer.concat([Buffer.from('[old](archive/old/a.md)\n'),large()]));
  assert.equal(apply(root).state,'compacted');const docs=expand(root);assert.equal(docs.length,4);
  assert.equal(apply(root).state,'unchanged');
});

test('under-budget legacy references alone do not create snapshots', t => {
  const root=fixture(t), memory=path.join(root,'.project_memory');fs.mkdirSync(path.join(memory,'archive/old'),{recursive:true});
  fs.writeFileSync(path.join(memory,'archive/old/page.md'),'Unseen constraints still apply.\n');
  fs.writeFileSync(source(root),'[old](archive/old/page.md)\n');
  assert.equal(apply(root).state,'unchanged');assert.equal(fs.existsSync(path.join(memory,'archive/auto')),false);
});

test('ambiguous identical pages never silently drop different relative dependencies', t => {
  const root=fixture(t), memory=path.join(root,'.project_memory');
  for(const folder of ['a','b']) {
    fs.mkdirSync(path.join(memory,'archive',folder),{recursive:true});
    fs.writeFileSync(path.join(memory,'archive',folder,'page.md'),'[required](leaf.md)\n');
    fs.writeFileSync(path.join(memory,'archive',folder,'leaf.md'),folder+' applies\n');
  }
  const bytes=Buffer.concat([Buffer.from('[a](archive/a/page.md)\n[b](archive/b/page.md)\n'),large()]);fs.writeFileSync(source(root),bytes);
  const result=apply(root);assert.equal(result.state,'blocked');assert.match(result.error,/Ambiguous/);assert.deepEqual(fs.readFileSync(source(root)),bytes);
});

test('BOM, mixed line endings, and a final unterminated line reconstruct exactly', t => {
  const root=fixture(t), bytes=Buffer.concat([Buffer.from('\ufeff# Original\nNOT 0.003 kg\r\n'),large(),Buffer.from('final 🧪 without newline')]);
  fs.writeFileSync(source(root),bytes);assert.equal(apply(root).state,'compacted');assert.ok(expand(root).some(doc=>doc.data.equals(bytes)));
});
