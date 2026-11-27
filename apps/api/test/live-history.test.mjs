import test from 'node:test'
import assert from 'node:assert/strict'
import {HISTORY_LIMIT,appendLiveSample,sanitizeLiveSample,readLiveHistory,liveHistoryReport} from '../../web/src/liveHistory.mjs'
const fixture=(elapsedMs=0,extra={})=>({mode:'native',phase:'streaming',elapsedMs,sessionId:'private-session',streamKey:'SECRET',serverUrl:'https://private.example',error:'SECRET',health:{health:'normal',payloadKbps:3000},bitrateAdvice:{currentKbps:3000},destinations:[{id:'private-id',phase:'sending',sentPackets:10,name:'private-name',url:'SECRET'}],...extra})
test('allowlist excludes secrets, identifiers and unknown strings even in stored data',()=>{
  const sample=sanitizeLiveSample(fixture(0,{phase:'streaming',health:{health:'SECRET'},timing:{encodedVideoLeadMs:-5,videoPackets:Infinity}}))
  assert.equal(sample.health.health,'unavailable')
  assert.equal(sample.timing.encodedVideoLeadMs,-5)
  assert.equal(sample.timing.videoPackets,null)
  const report=liveHistoryReport(readLiveHistory(JSON.stringify({schemaVersion:1,samples:[fixture()]})))
  for(const forbidden of ['SECRET','private-session','private-id','private-name','private.example','streamKey','serverUrl']) assert.ok(!report.includes(forbidden))
  assert.equal(JSON.parse(report).receiverVerified,false)
})
test('rejects rehearsal and invalid phases or times',()=>{
  assert.equal(sanitizeLiveSample(fixture(0,{mode:'rehearsal'})),null)
  assert.equal(sanitizeLiveSample(fixture(-1)),null)
  assert.equal(sanitizeLiveSample(fixture(1,{phase:'secret'})),null)
  assert.deepEqual(readLiveHistory('not json'),[])
})
test('samples at cadence, preserves changes and terminal results, rejects backward time',()=>{
  let h=appendLiveSample([],fixture())
  assert.equal(appendLiveSample(h,fixture(500)),h)
  h=appendLiveSample(h,fixture(10000))
  h=appendLiveSample(h,fixture(10500,{reconnectAttempt:1}))
  h=appendLiveSample(h,fixture(11000,{reconnectAttempt:1,phase:'failed'}))
  assert.equal(h.length,4)
  assert.equal(appendLiveSample(h,fixture(100)),h)
  assert.equal(h.at(-1).phase,'failed')
})
test('history is bounded in memory, storage and export',()=>{
  let h=[]
  for(let i=0;i<HISTORY_LIMIT+5;i++) h=appendLiveSample(h,fixture(i*10000))
  assert.equal(h.length,HISTORY_LIMIT)
  assert.equal(h[0].elapsedMs,50000)
  assert.equal(readLiveHistory(liveHistoryReport(h)).length,HISTORY_LIMIT)
  assert.deepEqual(readLiveHistory('x'.repeat(1024*1024+1)),[])
})
