import AVFoundation
import Foundation

private func micError(_ text: String) -> NSError {
    NSError(domain: "MooBeeProgramMicrophone", code: -1, userInfo: [NSLocalizedDescriptionKey: text])
}
private func micReport(_ error: Error, _ output: UnsafeMutablePointer<CChar>?, _ capacity: Int32) {
    guard let output, capacity > 0 else { return }
    let bytes = Array(error.localizedDescription.utf8.prefix(Int(capacity) - 1))
    for (i, byte) in bytes.enumerated() { output[i] = CChar(bitPattern: byte) }
    output[bytes.count] = 0
}

private final class ProgramMicrophone {
    let engine = AVAudioEngine()
    let condition = NSCondition()
    var queue = [Float]()
    var failure: String?
    var tapInstalled = false
    var stopped = false
    let maxFrames = 96_000
    // The default input device is used only after explicit user consent.
    // Resample to mono 48kHz, then duplicate to stereo. No device clock
    // synchronization is claimed; latency and drift require hardware QA.
    func start() throws {
        if AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined {
            let done = DispatchSemaphore(value: 0)
            AVCaptureDevice.requestAccess(for: .audio) { _ in done.signal() }
            guard done.wait(timeout: .now() + 30) == .success else { throw micError("Microphone permission request timed out") }
        }
        guard AVCaptureDevice.authorizationStatus(for: .audio) == .authorized else {
            throw micError("Microphone denied; enable MooBee in macOS Privacy & Security > Microphone")
        }
        let input = engine.inputNode
        let source = input.outputFormat(forBus: 0)
        guard source.sampleRate > 0, source.channelCount > 0,
              let target = AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: 1),
              let converter = AVAudioConverter(from: source, to: target) else {
            throw micError("Default microphone format unavailable")
        }
        input.installTap(onBus: 0, bufferSize: 1024, format: source) { [weak self] buffer, _ in
            guard let self else { return }
            let capacity = AVAudioFrameCount(ceil(Double(buffer.frameLength) * 48_000 / source.sampleRate) + 64)
            guard capacity > 0, capacity <= 48_000,
                  let converted = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else {
                self.fail("Microphone returned an invalid block"); return
            }
            var supplied = false
            var error: NSError?
            let status = converter.convert(to: converted, error: &error) { _, outStatus in
                if supplied { outStatus.pointee = .noDataNow; return nil }
                supplied = true
                outStatus.pointee = .haveData
                return buffer
            }
            if status == .error || error != nil { self.fail("Microphone sample-rate conversion failed"); return }
            guard let samples = converted.floatChannelData?[0] else { self.fail("Microphone PCM unavailable"); return }
            let n = Int(converted.frameLength)
            if n == 0 { return }
            self.condition.lock()
            defer { self.condition.broadcast(); self.condition.unlock() }
            if self.stopped || self.failure != nil { return }
            if self.queue.count + n > self.maxFrames {
                self.failure = "Microphone queue overflow; capture fell behind realtime"
                return
            }
            self.queue.append(contentsOf: UnsafeBufferPointer(start: samples, count: n))
        }
        tapInstalled = true
        engine.prepare()
        try engine.start()
    }

    func fail(_ message: String) {
        condition.lock()
        failure = message
        condition.broadcast()
        condition.unlock()
    }
    func readStereo(_ destination: UnsafeMutablePointer<Float>, frames: Int) throws {
        guard frames > 0, frames <= 4800 else { throw micError("Invalid microphone block size") }
        condition.lock()
        defer { condition.unlock() }
        let deadline = Date(timeIntervalSinceNow: 0.75)
        while queue.count < frames && failure == nil && !stopped {
            if !condition.wait(until: deadline) { break }
        }
        if let failure { throw micError(failure) }
        guard !stopped, queue.count >= frames else { throw micError("Microphone audio underrun; incomplete recording discarded") }
        for i in 0..<frames {
            destination[2*i] = queue[i]
            destination[2*i+1] = queue[i]
        }
        queue.removeFirst(frames)
    }
    func stop() {
        condition.lock(); stopped = true; condition.broadcast(); condition.unlock()
        if tapInstalled { engine.inputNode.removeTap(onBus: 0); tapInstalled = false }
        engine.stop()
    }
}

@_cdecl("moobee_program_mic_create")
public func moobee_program_mic_create(_ error: UnsafeMutablePointer<CChar>?, _ capacity: Int32) -> UnsafeMutableRawPointer? {
    let mic = ProgramMicrophone()
    do { try mic.start(); return Unmanaged.passRetained(mic).toOpaque() }
    catch let cause { mic.stop(); micReport(cause, error, capacity); return nil }
}
@_cdecl("moobee_program_mic_read")
public func moobee_program_mic_read(_ handle: UnsafeMutableRawPointer?, _ destination: UnsafeMutablePointer<Float>?,
                                   _ frames: Int, _ error: UnsafeMutablePointer<CChar>?, _ capacity: Int32) -> Int32 {
    guard let handle, let destination else { return -1 }
    do {
        try Unmanaged<ProgramMicrophone>.fromOpaque(handle).takeUnretainedValue().readStereo(destination, frames: frames)
        return 0
    } catch let cause { micReport(cause, error, capacity); return -1 }
}
@_cdecl("moobee_program_mic_destroy")
public func moobee_program_mic_destroy(_ handle: UnsafeMutableRawPointer?) {
    guard let handle else { return }
    Unmanaged<ProgramMicrophone>.fromOpaque(handle).takeRetainedValue().stop()
}
