// First-party native macOS MP4 recording adapter for MooBee Stream Studio.
// Compiled into the Tauri Rust binary, not installed/run as a separate process.
// macOS 15+: Apple ScreenCaptureKit + system-managed H.264/MP4 encoding.
// This scoped experiment records one display and optional system/microphone
// audio. It does NOT compose saved MooBee scenes or provide RTMP output.

import AVFoundation
import CoreMedia
import Foundation
import ScreenCaptureKit

/**
 * Thread-safe stop signal shared with Rust. This asks ScreenCaptureKit to
 * finalize the current session; it never publishes an unfinalized media file.
 * The existing Rust capture mutex still prevents concurrent record requests.
 */
private final class RecordingStopControl: @unchecked Sendable {
    private let lock = NSLock()
    private var active = false
    private var stopping = false

    func begin() {
        lock.lock()
        active = true
        stopping = false
        lock.unlock()
    }

    func requestStop() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard active else { return false }
        stopping = true
        return true
    }

    func shouldStop() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return stopping
    }

    func end() {
        lock.lock()
        active = false
        stopping = false
        lock.unlock()
    }
}

private let recordingStopControl = RecordingStopControl()

/// Non-blocking Rust-to-Swift cancellation signal. Returns 1 only when
/// a recording is active; stop completion is reported by the recording task.
@_cdecl("moobee_macos_request_stop_mp4")
public func moobee_macos_request_stop_mp4() -> Int32 {
    recordingStopControl.requestStop() ? 1 : 0
}

@available(macOS 15.0, *)
private final class RecordingCompletion: NSObject, SCRecordingOutputDelegate {
    private let lock = NSLock()
    private var finished: Result<Void, Error>?
    private var waiter: CheckedContinuation<Void, Error>?

    private func complete(_ result: Result<Void, Error>) {
        lock.lock()
        guard finished == nil else {
            lock.unlock()
            return
        }
        finished = result
        let continuation = waiter
        waiter = nil
        lock.unlock()
        continuation?.resume(with: result)
    }

    func recordingOutputDidStartRecording(_ recordingOutput: SCRecordingOutput) {
        // The first frame may arrive asynchronously; no success is claimed here.
    }

    func recordingOutputDidFinishRecording(_ recordingOutput: SCRecordingOutput) {
        complete(.success(()))
    }

    func recordingOutput(_ recordingOutput: SCRecordingOutput, didFailWithError error: Error) {
        complete(.failure(error))
    }

    func waitForFinalization() async throws {
        // Do not block the main actor: delegate callbacks may use its run loop.
        DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 15) { [self] in
            complete(.failure(NSError(
                domain: "MooBeeRecorder", code: -11,
                userInfo: [NSLocalizedDescriptionKey:
                    "ScreenCaptureKit did not finalize the MP4 within 15 seconds"]
            )))
        }
        try await withCheckedThrowingContinuation { continuation in
            lock.lock()
            if let finished {
                lock.unlock()
                continuation.resume(with: finished)
            } else {
                waiter = continuation
                lock.unlock()
            }
        }
    }
}

@available(macOS 15.0, *)
@MainActor
private func recordNativeMP4(
    url: URL, displayID: UInt32, seconds: Int,
    systemAudio: Bool, microphone: Bool
) async throws {
    let available = try await SCShareableContent.excludingDesktopWindows(
        false, onScreenWindowsOnly: false
    )
    // Use the stable CGDirectDisplayID obtained from XCap, never an array index.
    guard let display = available.displays.first(where: { $0.displayID == displayID }) else {
        throw NSError(domain: "MooBeeRecorder", code: -2,
                      userInfo: [NSLocalizedDescriptionKey:
                        "Selected macOS display ID unavailable; refresh capture devices"])
    }
    let filter = SCContentFilter(display: display, excludingWindows: [])
    let config = SCStreamConfiguration()

    // Keep the native system H.264 encoder inside a bounded 1080p30 experiment.
    let width = max(16, Int(display.width))
    let height = max(16, Int(display.height))
    let factor = min(1.0, 1920.0 / Double(width), 1080.0 / Double(height))
    config.width = max(16, Int(Double(width) * factor) / 2 * 2)
    config.height = max(16, Int(Double(height) * factor) / 2 * 2)
    config.minimumFrameInterval = CMTime(value: 1, timescale: 30)
    config.queueDepth = 5
    config.capturesAudio = systemAudio
    config.captureMicrophone = microphone

    let outputConfig = SCRecordingOutputConfiguration()
    outputConfig.outputURL = url
    outputConfig.outputFileType = .mp4
    outputConfig.videoCodecType = .h264
    let delegate = RecordingCompletion()
    let output = SCRecordingOutput(configuration: outputConfig, delegate: delegate)
    let stream = SCStream(filter: filter, configuration: config, delegate: nil)

    try stream.addRecordingOutput(output)
    try await stream.startCapture()
    do {
        // Check at quarter-second intervals for explicit user stop requests.
        // Recording duration remains bounded even without a stop request.
        let deadline = Date().addingTimeInterval(TimeInterval(seconds))
        while Date() < deadline && !recordingStopControl.shouldStop() {
            try await Task.sleep(nanoseconds: 250_000_000)
        }
    } catch {
        try? await stream.stopCapture()
        throw error
    }
    try await stream.stopCapture()
    // Await the encoding/muxing completion event, not just stopCapture.
    try await delegate.waitForFinalization()

    let size = try FileManager.default.attributesOfItem(atPath: url.path)[.size] as? NSNumber
    guard (size?.intValue ?? 0) >= 4096 else {
        throw NSError(domain: "MooBeeRecorder", code: -3,
                      userInfo: [NSLocalizedDescriptionKey:
                        "ScreenCaptureKit finished without a playable-sized MP4 file"])
    }
}

/// Separate the app-shell OS requirement from the newer native MP4 API.
/// Never infer macOS versions from browser WebKit user-agent strings.
/// The native runtime, not browser user-agent heuristics, supplies the exact
/// macOS version used to decide whether ScreenCaptureKit MP4 is available.
@_cdecl("moobee_macos_os_version")
public func moobee_macos_os_version(
    _ major: UnsafeMutablePointer<Int32>?,
    _ minor: UnsafeMutablePointer<Int32>?,
    _ patch: UnsafeMutablePointer<Int32>?
) {
    let version = ProcessInfo.processInfo.operatingSystemVersion
    major?.pointee = Int32(version.majorVersion)
    minor?.pointee = Int32(version.minorVersion)
    patch?.pointee = Int32(version.patchVersion)
}

@_cdecl("moobee_macos_mp4_supported")
public func moobee_macos_mp4_supported() -> Int32 {
    // macOS 15 uses SCRecordingOutput. Sonoma 14 uses AVAssetWriter and
    // SCStreamOutput to encode/mux H.264 in-process (hardware acceptance pending).
    if #available(macOS 14.0, *) { return 1 }
    return 0
}

private final class RecordingBridgeResult: @unchecked Sendable {
    let finished = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var failure: String?

    func complete(_ error: String?) {
        lock.lock()
        failure = error
        lock.unlock()
        finished.signal()
    }

    func error() -> String? {
        lock.lock()
        defer { lock.unlock() }
        return failure
    }
}

private func copyError(_ message: String, to buffer: UnsafeMutablePointer<CChar>?, size: Int32) {
    guard let buffer, size > 0 else { return }
    let bytes = Array(message.utf8.prefix(Int(size) - 1))
    for (index, byte) in bytes.enumerated() {
        buffer[index] = CChar(bitPattern: byte)
    }
    buffer[bytes.count] = 0
}

/// C ABI called from a blocking Rust worker; Swift's ScreenCaptureKit work
/// executes on the main actor without an independent helper executable.
/// Returns 0 on completion. A nonzero result must never publish the temp file.
@_cdecl("moobee_macos_record_mp4")
public func moobee_macos_record_mp4(
    output: UnsafePointer<CChar>?,
    displayID: UInt32,
    durationSeconds: Int32,
    systemAudio: Int32,
    microphone: Int32,
    errorText: UnsafeMutablePointer<CChar>?,
    errorLength: Int32
) -> Int32 {
    guard let output, (3...120).contains(durationSeconds),
          displayID != 0 else {
        copyError("Invalid native MP4 capture path, display or duration", to: errorText, size: errorLength)
        return -1
    }
    let filePath = String(cString: output)
    guard filePath.hasSuffix(".mp4.part"), filePath.utf8.count < 4096 else {
        copyError("Native recorder requires a private temporary MP4 path", to: errorText, size: errorLength)
        return -1
    }
    guard #available(macOS 14.0, *) else {
        copyError("Native MP4 requires macOS 14 or newer", to: errorText, size: errorLength)
        return -1
    }

    let result = RecordingBridgeResult()
    recordingStopControl.begin()
    Task { @MainActor in
        defer { recordingStopControl.end() }
        do {
            if #available(macOS 15.0, *) {
                try await recordNativeMP4(
                    url: URL(fileURLWithPath: filePath),
                    displayID: displayID,
                    seconds: Int(durationSeconds),
                    systemAudio: systemAudio != 0,
                    microphone: microphone != 0
                )
            } else if #available(macOS 14.0, *) {
                try await recordSonomaMP4(
                    url: URL(fileURLWithPath: filePath),
                    displayID: displayID,
                    seconds: Int(durationSeconds),
                    systemAudio: systemAudio != 0,
                    microphone: microphone != 0,
                    shouldStop: { recordingStopControl.shouldStop() }
                )
            }
            result.complete(nil)
        } catch {
            result.complete(error.localizedDescription)
        }
    }

    if result.finished.wait(timeout: .now() + .seconds(Int(durationSeconds) + 40)) == .timedOut {
        copyError("Native MP4 capture timed out", to: errorText, size: errorLength)
        return -2
    }
    if let failure = result.error() {
        copyError(failure, to: errorText, size: errorLength)
        return -3
    }
    return 0
}
