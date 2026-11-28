import test from 'node:test'
import assert from 'node:assert/strict'
import {HISTORY_LIMIT,appendLiveSample,sanitizeLiveSample,readLiveHistory,liveHistoryReport,updateSessionHistory} from '../../web/src/liveHistory.mjs'
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
  assert.deepEqual(readLiveHistory('x'.repeat(8*1024*1024+1)),[])
})

test('session isolation, terminal classification and no session identity in export',()=>{
  const first='11111111-1111-4111-8111-111111111111',second='22222222-2222-4222-8222-222222222222'
  let h=updateSessionHistory({sessionId:null,samples:[]},fixture(100,{sessionId:first}))
  h=updateSessionHistory(h,fixture(150,{sessionId:first,phase:'stopped',nativeOutcome:'stoppedEarly'}))
  assert.equal(h.samples.length,2)
  assert.equal(h.samples.at(-1).nativeOutcome,'stoppedEarly')
  assert.equal(updateSessionHistory(h,fixture(200,{mode:'rehearsal',sessionId:second})),h)
  h=updateSessionHistory(h,fixture(0,{sessionId:second}))
  assert.equal(h.samples.length,1)
  assert.equal(h.samples[0].elapsedMs,0)
  assert.ok(!liveHistoryReport(h.samples).includes(second))
  assert.equal(updateSessionHistory(h,fixture(0,{sessionId:'invalid'})),h)
})

test('authorized output and unsupported/uncertain encoder transitions are retained immediately',()=>{
  let h=appendLiveSample([],fixture(0,{destinations:[{phase:'authorized'}]}))
  assert.equal(h[0].destinations[0].phase,'authorized')
  h=appendLiveSample(h,fixture(100,{destinations:[{phase:'authorized'}],bitrateAdvice:{currentKbps:3000,inplaceUnsupported:true}}))
  h=appendLiveSample(h,fixture(200,{destinations:[{phase:'authorized'}],bitrateAdvice:{currentKbps:3000,inplaceUnsupported:true,inplaceUncertain:true}}))
  assert.equal(h.length,3)
  assert.equal(h.at(-1).bitrateAdvice.inplaceUncertain,true)
})
