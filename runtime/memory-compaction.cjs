'use strict';
// Physical pagination only. No scientific statement is accepted or retired here.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const BUDGETS = Object.freeze(Object.fromEntries(Object.entries({
  PROJECT_STATE:16, SESSION_HANDOFF:12, DECISIONS:32, MODEL_DESIGN:32,
  DATA_CATALOG:32, RESULTS:32, REFERENCES:32, EXPERIMENTS:48, DOCUMENTS:24,
}).map(([stem,kib])=>[stem+'.md',kib*1024])));
const LEASE = '.tunneldock/memory-compaction.lock';
const STATUS = '.project_memory/AUTO_COMPACTION_STATUS.json';
const sha = data => crypto.createHash('sha256').update(data).digest('hex');
const json = value => Buffer.from(JSON.stringify(value,null,2)+'\n');
const fail = (message,state='blocked') => {throw Object.assign(Error(message),{state});};

function checked(absolute) {
  const resolved=path.resolve(absolute), parsed=path.parse(resolved);
  let at=parsed.root;
  for(const part of resolved.slice(parsed.root.length).split(path.sep).filter(Boolean)) {
    at=path.join(at,part);
    try {if(fs.lstatSync(at).isSymbolicLink()) fail('Symlink/reparse path rejected: '+at);}
    catch(e) {if(e.code!=='ENOENT') throw e;}
  }
  return resolved;
}
function safe(root,relative) {
  if(typeof relative!=='string' || !relative || path.isAbsolute(relative) || /[\\:\x00-\x1f]/.test(relative)
      || relative.split('/').some(p=>p==='..' || p==='.' || !p || /[. ]$/.test(p))) fail('Unsafe relative path: '+relative);
  const file=path.resolve(root,...relative.split('/'));
  if(!path.relative(root,file) || path.relative(root,file).startsWith('..'+path.sep)) fail('Path escapes root');
  return checked(file);
}
function stat(file) {
  try {return fs.lstatSync(checked(file));} catch(e) {if(e.code==='ENOENT') return null;throw e;}
}
function same(a,b) {
  return a && b && a.dev===b.dev && a.ino===b.ino && a.size===b.size && a.mtimeMs===b.mtimeMs && a.ctimeMs===b.ctimeMs;
}
function read(file,maxBytes=64*1024*1024) {
  const before=stat(file);
  if(!before || !before.isFile() || before.nlink!==1) fail('Expected regular, non-hardlinked file: '+file);
  if(before.size>maxBytes) fail('Read resource limit exceeded: '+file);
  const fd=fs.openSync(checked(file),fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0));
  try {
    if(!same(before,fs.fstatSync(fd))) fail('Source changed before read: '+file,'busy');
    const data=fs.readFileSync(fd);
    if(!same(before,fs.fstatSync(fd)) || !same(before,stat(file))) fail('Source changed during read: '+file,'busy');
    return {data,stat:before,sha256:sha(data)};
  } finally {fs.closeSync(fd);}
}
function utf8(data) {
  if(data.includes(0) || !Buffer.from(data.toString('utf8')).equals(data)) fail('Canonical text must be valid UTF-8 without NUL');
  return data.toString('utf8');
}
function identity(root,projectId) {
  checked(root);
  if(!fs.lstatSync(root).isDirectory()) fail('Project root must be a directory');
  const memory=safe(root,'.project_memory');
  if(!fs.lstatSync(memory).isDirectory()) fail('Missing .project_memory directory');
  const binding=read(safe(root,'.tunneldock/session_binding.json'),1024*1024);
  const value=JSON.parse(binding.data);
  if(typeof value.project_id!=='string' || !/^[a-zA-Z0-9_-]+$/.test(value.project_id)
      || typeof value.cwd!=='string' || !path.isAbsolute(value.cwd)
      || fs.realpathSync(checked(value.cwd))!==fs.realpathSync(root)
      || (projectId!==undefined && value.project_id!==projectId)) fail('Session binding identity mismatch');
  return {...binding,project_id:value.project_id};
}
function policy(root) {
  const file=safe(root,'.tunneldock/memory-policy.json');
  if(!stat(file)) return false;
  const value=JSON.parse(read(file,1024*1024).data);
  if(!value || typeof value!=='object' || Array.isArray(value)) fail('Invalid memory policy');
  return value.auto_compact===true;
}
function acquire(root) {
  const file=safe(root,LEASE), token=crypto.randomUUID();
  let fd;
  try {fd=fs.openSync(file,'wx',0o600);}
  catch(e) {if(e.code==='EEXIST') fail('Existing project writer lease; never automatically stolen','busy');throw e;}
  try {fs.writeFileSync(fd,json({version:1,pid:process.pid,token}));fs.fsyncSync(fd);}
  catch(e) {fs.closeSync(fd);fs.unlinkSync(file);throw e;}
  fs.closeSync(fd);
  return {file,token};
}
function assertLease(lease) {
  if(JSON.parse(read(lease.file,4096).data).token!==lease.token) fail('Project lease changed','busy');
}
function release(lease) {
  assertLease(lease);fs.unlinkSync(lease.file);
}
function syncDirectory(dir) {
  // Windows does not expose portable directory fsync through Node.
  if(process.platform==='win32') return;
  const fd=fs.openSync(checked(dir),'r');try {fs.fsyncSync(fd);} finally {fs.closeSync(fd);}
}
function stage(file,data,mode=0o600) {
  checked(path.dirname(file));
  const temp=file+'.'+crypto.randomUUID()+'.tmp';
  const fd=fs.openSync(checked(temp),'wx',mode);
  try {fs.writeFileSync(fd,data);fs.fsyncSync(fd);}
  catch(e) {fs.closeSync(fd);fs.unlinkSync(temp);throw e;}
  fs.closeSync(fd);return temp;
}
function immutable(root,relative,data) {
  const file=safe(root,relative);
  fs.mkdirSync(checked(path.dirname(file)),{recursive:true});
  if(stat(file)) {
    if(!read(file).data.equals(data)) fail('Immutable archive/manifest checksum mismatch: '+relative);
    return;
  }
  const temp=stage(file,data);
  try {
    // Atomic no-clobber publication; unlike rename this cannot replace a raced object.
    try {fs.linkSync(checked(temp),safe(root,relative));}
    catch(e) {if(e.code!=='EEXIST') throw e;}
  } finally {fs.unlinkSync(checked(temp));}
  syncDirectory(path.dirname(file));
  if(!read(safe(root,relative)).data.equals(data)) fail('Immutable archive verification failed: '+relative);
}
function publish(root,relative,data) {
  const file=safe(root,relative);
  if(stat(file) && read(file).data.equals(data)) return;
  const temp=stage(file,data);
  try {fs.renameSync(checked(temp),safe(root,relative));syncDirectory(path.dirname(file));}
  finally {if(stat(temp)) fs.unlinkSync(checked(temp));}
}

function archivePath(file,digest) {
  const stem=path.posix.basename(file,'.md').replace(/[^a-zA-Z0-9_-]/g,'_');
  return 'archive/auto/'+stem+'/'+digest+'.md';
}
function references(text,origin) {
  const found=new Set(), inArchive=origin.startsWith('archive/');
  const add = raw => {
    let target;
    try {target=decodeURIComponent(raw.split('#')[0]);} catch {fail('Invalid encoded continuation path');}
    if(target.startsWith('.project_memory/')) target=target.slice(16);
    if(!target || /^[a-z][a-z0-9+.-]*:/i.test(target)) return;
    if(!target.startsWith('archive/') && !inArchive) return;
    if(!/\.md$/i.test(target)) return;
    if(path.posix.isAbsolute(target) || /[\\:\x00-\x1f]/.test(target)) fail('Unsafe continuation path: '+raw);
    target=target.startsWith('archive/')?path.posix.normalize(target):path.posix.join(path.posix.dirname(origin),target);
    if(!target.startsWith('archive/')) fail('Continuation escapes archive');
    found.add(target);
  };
  // Inline Markdown links, reference definitions, and explicit archive paths in
  // older backtick/plain-text continuation notices are all treated as required.
  for(const match of text.matchAll(/\]\(\s*(?:<([^>]+)>|([^\s)]+))(?:\s+[^)]*)?\)/g)) add(match[1]||match[2]);
  for(const match of text.matchAll(/^[ \t]*\[[^\]\r\n]+\]:[ \t]*(?:<([^>\r\n]+)>|(\S+))/gm)) add(match[1]||match[2]);
  for(const match of text.matchAll(/(?:\.project_memory\/)?archive\/[^\s`<>"'()\[\]]+\.md/g)) add(match[0]);
  return [...found].sort();
}
function graph(root,limits) {
  const nodes=new Map(), paths=new Map(); let total=0;
  function visit(origin,snapshot) {
    if(paths.has(origin)) return paths.get(origin);
    const original=snapshot || read(safe(root,'.project_memory/'+origin),limits.maxFileBytes);
    const text=utf8(original.data), automatic=/^archive\/auto\/[^/]+\/([a-f0-9]{64})\.md$/.exec(origin);
    if(automatic && automatic[1]!==original.sha256) fail('Archive checksum mismatch: '+origin);
    const destination=automatic?origin:archivePath(origin,original.sha256);
    paths.set(origin,destination);
    if(nodes.has(destination)) {
      if(!automatic && nodes.get(destination).source_path!==origin) fail('Ambiguous identical archive pages have different reference origins');
      return destination;
    }
    total+=original.data.length;
    if(nodes.size>=limits.maxObjects || total>limits.maxTotalBytes) fail('Expansion resource limit exceeded; full review remains blocked');
    const node={archive:destination,sha256:original.sha256,bytes:original.data.length,data:original.data,dependencies:[],references:[],source_path:origin};
    nodes.set(destination,node);
    const sidecar=safe(root,'.project_memory/'+destination+'.json');
    if(automatic) {
      const manifest=JSON.parse(read(sidecar,limits.maxFileBytes).data);
      if(manifest.version!==1 || manifest.sha256!==node.sha256 || manifest.bytes!==node.bytes || !Array.isArray(manifest.dependencies)
          || !Array.isArray(manifest.references) || typeof manifest.source_path!=='string') fail('Invalid continuation manifest: '+origin);
      safe(root,'.project_memory/'+manifest.source_path);
      node.source_path=manifest.source_path;
      const expected=references(text,node.source_path);
      if(JSON.stringify(expected)!==JSON.stringify(manifest.references.map(ref=>ref.path))) fail('Manifest omits or changes original reference paths: '+origin);
      for(const ref of manifest.references) {
        if(!manifest.dependencies.includes(ref.archive) || (ref.path.startsWith('archive/auto/') && ref.path!==ref.archive)) fail('Invalid continuation mapping');
      }
      node.references=manifest.references;
      for(const entry of manifest.dependencies) {
        if(typeof entry!=='string' || !/^archive\/auto\/[^/]+\/[a-f0-9]{64}\.md$/.test(entry)) fail('Invalid manifest dependency');
        node.dependencies.push(visit(entry));
      }
      // Legacy relative references are resolved by the immutable manifest, not
      // relative to their new content-addressed location.
      for(const ref of expected.filter(p=>p.startsWith('archive/auto/'))) {
        if(!manifest.dependencies.includes(ref)) fail('Manifest omits a required continuation: '+ref);
      }
    } else {
      for(const entry of references(text,origin)) {
        const archive=visit(entry);node.dependencies.push(archive);node.references.push({path:entry,archive});
      }
    }
    node.dependencies=[...new Set(node.dependencies)].sort();
    node.manifest={version:1,sha256:node.sha256,bytes:node.bytes,source_path:node.source_path,references:node.references,dependencies:node.dependencies,
      expansion_required:true,unseen_content_still_applies:true,semantic_acceptance:false};
    if(automatic && !read(sidecar,limits.maxFileBytes).data.equals(json(node.manifest))) fail('Noncanonical or corrupt continuation manifest: '+origin);
    return destination;
  }
  return {nodes,visit};
}
function bounded(data,archive,budget) {
  const notice=Buffer.from('<!-- TunnelDock required-continuation: '+archive+' -->\n'
    +'# Required canonical continuation\n\n'
    +'Unseen content still applies. This excerpt does not preserve all active constraints.\n'
    +'Full expansion is mandatory before decisive research review. No semantic acceptance or obsolescence is implied.\n'
    +'Read the exact original ['+archive+']('+archive+') and recursively expand its machine manifest `'+archive+'.json`.\n'
    +'The following is unchanged original text, ending at a complete line boundary:\n\n');
  const available=budget-notice.length;let end=0;
  for(let i=0;i<Math.min(available,data.length);i++) {
    if(data[i]===10 || (data[i]===13 && data[i+1]!==10)) end=i+1;
  }
  if(!end) fail('No complete original line fits the canonical byte budget');
  return {data:Buffer.concat([notice,data.subarray(0,end)]),notice_bytes:notice.length,retained_bytes:end};
}
function optionsFor(options) {
  const result={quietMs:5000,maxFileBytes:64*1024*1024,maxTotalBytes:256*1024*1024,maxObjects:4096,...options};
  for(const key of ['quietMs','maxFileBytes','maxTotalBytes','maxObjects']) {
    if(!Number.isSafeInteger(result[key]) || result[key]<(key==='quietMs'?0:1)) fail('Invalid option: '+key);
  }
  for(const key of ['apply','policyEnabledOnly']) if(result[key]!==undefined && typeof result[key]!=='boolean') fail('Invalid option: '+key);
  for(const [key,ceiling] of Object.entries({maxFileBytes:64*1024*1024,maxTotalBytes:256*1024*1024,maxObjects:4096})) {
    if(result[key]>ceiling) fail('Resource option exceeds safety ceiling: '+key);
  }
  return result;
}
function prepare(root,options) {
  const tree=graph(root,options), files=[];
  for(const [file,budget] of Object.entries(BUDGETS)) {
    const absolute=safe(root,'.project_memory/'+file);
    if(!stat(absolute)) continue;
    const original=read(absolute,options.maxFileBytes), over=original.data.length>budget;
    if(over && Date.now()-Math.max(original.stat.mtimeMs,original.stat.ctimeMs)<options.quietMs) fail('Recent canonical writer: '+file,'busy');
    // Validate continuations even on a no-change refresh; missing/corrupt cold
    // content must never produce a healthy status.
    let archive;
    if(over) archive=tree.visit(file,original);
    else for(const ref of references(utf8(original.data),file).filter(ref=>ref.startsWith('archive/auto/'))) tree.visit(ref);
    files.push({file,budget,original,archive,replacement:over?bounded(original.data,archive,budget):null});
  }
  return {tree,files};
}
function verifySnapshot(root,row,quietMs) {
  const current=read(safe(root,'.project_memory/'+row.file));
  if(current.sha256!==row.original.sha256 || !same(current.stat,row.original.stat)) fail('Canonical source changed before replacement: '+row.file,'busy');
  if(Date.now()-Math.max(current.stat.mtimeMs,current.stat.ctimeMs)<quietMs) fail('Recent canonical writer: '+row.file,'busy');
}
function replace(root,row,binding,lease,options) {
  const relative='.project_memory/'+row.file, file=safe(root,relative);
  const temp=stage(file,row.replacement.data,row.original.stat.mode & 0o777);
  try {
    assertLease(lease);
    if(identity(root,options.projectId).sha256!==binding.sha256) fail('Session binding changed','busy');
    verifySnapshot(root,row,options.quietMs);
    fs.renameSync(checked(temp),safe(root,relative));
  } finally {if(stat(temp)) fs.unlinkSync(checked(temp));}
}
function compact(projectRoot,options={}) {
  const report={version:1,state:'blocked',changed_files:[],files:[],archives:[],semantic_acceptance:false,expansion_required:true};
  let root,lease,enabled=false;
  try {
    options=optionsFor(options);root=checked(projectRoot);
    const binding=identity(root,options.projectId); report.project_id=binding.project_id;
    const optedIn=policy(root);
    enabled=options.apply===undefined?optedIn:options.apply;
    if(options.policyEnabledOnly && !optedIn) return {...report,state:'disabled'};
    const statusFile=safe(root,STATUS);if(stat(statusFile)) read(statusFile);
    lease=acquire(root);
    const {tree,files}=prepare(root,options);
    report.files=files.map(row=>({file:row.file,budget_bytes:row.budget,before_bytes:row.original.data.length,
      after_bytes:row.original.data.length,planned_after_bytes:row.replacement?row.replacement.data.length:row.original.data.length}));
    report.archives=[...tree.nodes.values()].map(node=>({path:node.archive,sha256:node.sha256,bytes:node.bytes,manifest:node.archive+'.json'}));
    const pending=files.filter(row=>row.replacement);
    if(!enabled) {report.state=pending.length?'dry_run':'unchanged';}
    else {
      // Every dependency and its manifest is durable before any canonical pointer.
      if(pending.length) {
        for(const node of tree.nodes.values()) immutable(root,'.project_memory/'+node.archive,node.data);
        for(const node of tree.nodes.values()) immutable(root,'.project_memory/'+node.archive+'.json',json(node.manifest));
      }
      for(const row of pending) verifySnapshot(root,row,options.quietMs);
      for(const row of pending) {
        for(const node of tree.nodes.values()) {
          if(read(safe(root,'.project_memory/'+node.archive)).sha256!==node.sha256
              || !read(safe(root,'.project_memory/'+node.archive+'.json')).data.equals(json(node.manifest))) fail('Archive changed before replacement');
        }
        replace(root,row,binding,lease,options);
        report.changed_files.push({file:row.file,before_bytes:row.original.data.length,after_bytes:row.replacement.data.length,
          archive:row.archive,sha256:row.original.sha256,notice_bytes:row.replacement.notice_bytes,retained_bytes:row.replacement.retained_bytes});
        report.files.find(file=>file.file===row.file).after_bytes=row.replacement.data.length;
        syncDirectory(path.join(root,'.project_memory'));
      }
      report.state=pending.length?'compacted':'unchanged';
    }
  } catch(e) {report.state=e.state||'blocked';report.error=String(e.message||e);}
  finally {
    if(lease) {
      try {
        assertLease(lease);
        if(enabled) publish(root,STATUS,json(report));
      } catch(e) {report.state='blocked';report.status_error=String(e.message||e);}
      try {release(lease);} catch(e) {report.state='blocked';report.lease_error=String(e.message||e);}
    }
  }
  return report;
}

// Return separate exact documents, never concatenate or silently discard earlier
// generations. Callers must consume every document and handle failures as incomplete.
function expand(projectRoot,file='PROJECT_STATE.md',options={}) {
  const root=checked(projectRoot);identity(root,options.projectId);
  if(!Object.hasOwn(BUDGETS,file)) fail('Unknown canonical memory file');
  const limits=optionsFor(options), current=read(safe(root,'.project_memory/'+file),limits.maxFileBytes);
  const tree=graph(root,limits);
  for(const ref of references(utf8(current.data),file)) tree.visit(ref);
  return [{path:file,sha256:current.sha256,data:current.data},
    ...[...tree.nodes.values()].map(node=>({path:node.archive,sha256:node.sha256,data:node.data}))];
}
function main(argv) {
  const [root,...flags]=argv;
  if(!root || flags.some(flag=>!['--apply','--policy-enabled-only'].includes(flag))) fail('Usage: node memory-compaction.cjs <root> [--apply] [--policy-enabled-only]');
  return compact(root,{...(flags.includes('--apply')?{apply:true}:{}),policyEnabledOnly:flags.includes('--policy-enabled-only')});
}
module.exports={compact,expand,BUDGETS,LEASE,main};
if(require.main===module) {
  try {const result=main(process.argv.slice(2));process.stdout.write(JSON.stringify(result)+'\n');if(['blocked','busy'].includes(result.state)) process.exitCode=1;}
  catch(e) {process.stderr.write(JSON.stringify({state:'blocked',error:e.message})+'\n');process.exitCode=1;}
}
