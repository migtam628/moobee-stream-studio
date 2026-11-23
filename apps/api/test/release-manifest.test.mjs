import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { manifestForArtifacts } from '../../../scripts/build-release-manifest.mjs'

const tag = 'v0.3.8-alpha.2'
const commit = 'a'.repeat(40)
const names = [
  'macos-arm64-MooBee_0.3.8_aarch64.dmg',
  'macos-x64-MooBee_0.3.8_x64.dmg',
  'windows-x64-MooBee_0.3.8_x64-setup.exe',
  'linux-x64-MooBee_0.3.8_amd64.deb',
  'moobee-stream-studio-android-arm64-debug.apk',
  'moobee-stream-studio-web-abc1234.tar.gz'
]

function fixture(files = names) {
  const dir = mkdtempSync(join(tmpdir(), 'moobee-release-test-'))
  for (const name of files) {
    mkdirSync(join(dir, 'nested'), { recursive: true })
    writeFileSync(join(dir, 'nested', name), name)
  }
  return dir
}

test('release manifest enumerates all requested target packages with hashes', () => {
  const dir = fixture()
  try {
    const m = manifestForArtifacts(dir, tag, commit)
    assert.equal(m.artifacts.length, 6)
    assert.equal(m.sourceCommit, commit)
    assert.equal(m.releaseKind, 'experimental-prerelease')
    assert.equal(m.installableAppUpdateSigned, false)
    assert.equal(m.hardwareAcceptanceVerified, false)
    assert.ok(m.artifacts.every(a => /^[0-9a-f]{64}$/.test(a.sha256)))
  } finally { rmSync(dir, { recursive: true, force: true }) }
})

test('missing installer, duplicate names, empty files and invalid versions fail closed', () => {
  const dir = fixture(names.slice(0, -1))
  try {
    assert.throws(() => manifestForArtifacts(dir, tag, commit), /Missing required/)
    assert.throws(() => manifestForArtifacts(dir, 'v0.3.8', commit), /prerelease/)
    assert.throws(() => manifestForArtifacts(dir, tag, 'abc'), /commit SHA/)
  } finally { rmSync(dir, { recursive: true, force: true }) }

  const dup = fixture()
  try {
    writeFileSync(join(dup, names[0]), 'duplicate')
    assert.throws(() => manifestForArtifacts(dup, tag, commit), /Duplicate asset/)
  } finally { rmSync(dup, { recursive: true, force: true }) }

  const empty = fixture()
  try {
    writeFileSync(join(empty, 'nested', names[0]), '')
    assert.throws(() => manifestForArtifacts(empty, tag, commit), /Empty release artifact/)
  } finally { rmSync(empty, { recursive: true, force: true }) }
})

test('explicit alpha-only Android throttle omission is labeled incomplete, never hardware-ready', () => {
  const withoutAndroid = fixture(names.filter(name => !name.endsWith('.apk')))
  try {
    assert.throws(() => manifestForArtifacts(withoutAndroid, tag, commit), /Missing required/)
    const result = manifestForArtifacts(withoutAndroid, tag, commit, { androidThrottled: true })
    assert.equal(result.partialPlatformDistribution, true)
    assert.deepEqual(result.unavailableTargets.map(x => x.target), ['android-arm64-debug'])
    assert.equal(result.hardwareAcceptanceVerified, false)
    assert.equal(result.installableAppUpdateSigned, false)
    assert.equal(result.artifacts.length, 5)
    assert.throws(() => manifestForArtifacts(withoutAndroid, 'v0.4.0-beta.1', commit, { androidThrottled: true }), /Only experimental alpha/)
    assert.throws(() => manifestForArtifacts(withoutAndroid, 'v0.4.0-rc.1', commit, { androidThrottled: true }), /Only experimental alpha/)
  } finally { rmSync(withoutAndroid, { recursive: true, force: true }) }
  const complete = fixture()
  try {
    const full = manifestForArtifacts(complete, tag, commit)
    assert.equal(full.partialPlatformDistribution, false)
    assert.deepEqual(full.unavailableTargets, [])
    assert.throws(() => manifestForArtifacts(complete, tag, commit, { androidThrottled: true }), /override is invalid/)
  } finally { rmSync(complete, { recursive: true, force: true }) }
})
