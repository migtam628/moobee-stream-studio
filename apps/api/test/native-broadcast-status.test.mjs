import test from 'node:test'
import assert from 'node:assert/strict'
import {broadcastIndicator} from '../../web/src/nativeBroadcastStatus.mjs'
test('native phase overrides idle UI without claiming receiver success',()=>{
  for(const phase of ['starting','streaming','reconnecting','stopping']){
    const s=broadcastIndicator({mode:'native',phase,elapsedMs:2500,lateFrames:2,health:{health:'normal',payloadKbps:3000},destinations:[{phase:'sending'},{phase:'stopped'}]},'available')
    assert.equal(s.active,true);assert.equal(s.seconds,2);assert.equal(s.outputs,1);assert.equal(s.late,2)
    assert.equal(s.kbps,phase==='streaming'?3000:null)
    assert.match(s.label,/Native test/)
  }
  for(const phase of ['stopped','failed'])assert.equal(broadcastIndicator({mode:'native',phase},'available'),null)
  assert.equal(broadcastIndicator({mode:'rehearsal',phase:'streaming'},'available'),null)
})
test('poll failures or stale readings never invent current rates',()=>{
  assert.equal(broadcastIndicator(null,'loading').uncertain,true)
  assert.equal(broadcastIndicator(null,'unavailable').uncertain,true)
  const s=broadcastIndicator({mode:'native',phase:'streaming',elapsedMs:-1,lateFrames:NaN,health:{health:'stale',payloadKbps:6000}},'available')
  assert.equal(s.seconds,null);assert.equal(s.late,null);assert.equal(s.kbps,null)
})
