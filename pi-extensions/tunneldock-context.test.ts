import {it,expect} from 'vitest';
import {engineArgs} from './tunneldock-context';
it('uses argv, not shell interpolation',()=>{
 expect(engineArgs('D:/repo',{action:'search',query:'x; touch /tmp/no'})).toEqual(['search','D:/repo','x; touch /tmp/no','all']);
});
it('requires an exact digest and nonnegative offset',()=>{
 expect(()=>engineArgs('D:/repo',{action:'read',sha256:'../x'})).toThrow();
 expect(()=>engineArgs('D:/repo',{action:'read',sha256:'a'.repeat(64),offset:-1})).toThrow();
});
it('paginates full evidence instead of rereading a whole result',()=>{
 expect(engineArgs('D:/repo',{action:'read',sha256:'a'.repeat(64),offset:4096}).slice(-3)).toEqual(['4096','4096','memory']);
});
