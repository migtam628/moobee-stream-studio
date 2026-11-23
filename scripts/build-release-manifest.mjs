#!/usr/bin/env node
// Build a machine-readable, SHA-256-verified GitHub prerelease manifest.
// Does not claim executable signing or hardware/media compatibility.
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, basename } from 'node:path'
import { fileURLToPath } from 'node:url'

export function manifestForArtifacts(dir, tag, commit, options = {}) {
  const androidThrottled = options.androidThrottled === true
  if (androidThrottled && !/^v\d+\.\d+\.\d+-alpha\.\d+$/.test(tag)) {
    throw new Error('Only experimental alpha releases can omit an Android test APK due to Maven HTTP 429')
  }
  if (!/^v\d+\.\d+\.\d+-(alpha|beta|rc)\.\d+$/.test(tag)) {
    throw new Error('A valid prerelease tag is required')
  }
  if (!/^[0-9a-f]{40}$/.test(commit)) {
    throw new Error('A full source commit SHA is required')
  }
  const paths = readdirSync(dir, { recursive: true })
    .map(name => join(dir, name))
    .filter(name => statSync(name).isFile())
    .sort()
  if (paths.length === 0) throw new Error('No release artifacts found')
  const seen = new Set()
  const artifacts = paths.map(path => {
    const name = basename(path)
    if (seen.has(name)) throw new Error(`Duplicate asset name: ${name}`)
    seen.add(name)
    const file = readFileSync(path)
    if (file.length === 0) throw new Error(`Empty release artifact: ${name}`)
    return {
      name,
      bytes: file.length,
      sha256: createHash('sha256').update(file).digest('hex'),
      artifactPath: relative(dir, path).replaceAll('\\', '/')
    }
  })
  const required = [
    /^macos-arm64-.*\.dmg$/,
    /^macos-x64-.*\.dmg$/,
    /^windows-x64-.*setup\.exe$/,
    /^linux-x64-.*\.deb$/,
    /^moobee-stream-studio-web-.*\.tar\.gz$/
  ]
  const androidPattern = /^moobee-stream-studio-android-arm64-debug\.apk$/
  const hasAndroid = artifacts.some(item => androidPattern.test(item.name))
  if (!hasAndroid && !androidThrottled) {
    throw new Error('Missing required alpha artifact: Android debug APK')
  }
  if (hasAndroid && androidThrottled) {
    throw new Error('Android throttling override is invalid when an APK is present')
  }
  for (const pattern of required) {
    if (!artifacts.some(item => pattern.test(item.name))) {
      throw new Error(`Missing required alpha artifact: ${pattern}`)
    }
  }
  return {
    schemaVersion: 1,
    releaseTag: tag,
    sourceCommit: commit,
    releaseKind: 'experimental-prerelease',
    installableAppUpdateSigned: false,
    hardwareAcceptanceVerified: false,
    generatedAt: new Date().toISOString(),
    // An alpha missing the optional Android *test* package must disclose it.
    partialPlatformDistribution: !hasAndroid,
    unavailableTargets: hasAndroid ? [] : [{
      target: 'android-arm64-debug',
      reason: 'Maven Central HTTP 429 dependency throttling during Android build',
      installable: false
    }],
    artifacts
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    const [dir, tag, sha, flag] = process.argv.slice(2)
    if (flag && flag !== '--android-throttled') throw new Error('Unknown release manifest option')
    if (!dir || !tag || !sha) throw new Error('Usage: node scripts/build-release-manifest.mjs release-assets v0.3.8-alpha.2 COMMIT_SHA')
    process.stdout.write(JSON.stringify(manifestForArtifacts(dir, tag, sha, {
      androidThrottled: flag === '--android-throttled'
    }), null, 2) + '\n')
  } catch (error) {
    console.error((error instanceof Error ? error.message : String(error)))
    process.exitCode = 1
  }
}
