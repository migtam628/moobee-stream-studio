import AVFoundation
import Foundation
import CoreMedia
import AudioToolbox
import ScreenCaptureKit

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
    static func authorize() throws {
        if AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined {
            let done = DispatchSemaphore(value: 0)
            AVCaptureDevice.requestAccess(for: .audio) { _ in done.signal() }
            guard done.wait(timeout: .now() + 30) == .success else { throw micError("Microphone permission request timed out") }
        }
        guard AVCaptureDevice.authorizationStatus(for: .audio) == .authorized else {
            throw micError("Microphone denied; enable MooBee in macOS Privacy & Security > Microphone")
        }
    }
    func start() throws {
        try Self.authorize()
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

@_cdecl("moobee_program_mic_authorize")
public func moobee_program_mic_authorize(_ error: UnsafeMutablePointer<CChar>?, _ capacity: Int32) -> Int32 {
    do { try ProgramMicrophone.authorize(); return 0 }
    catch let cause { micReport(cause, error, capacity); return -1 }
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

/* Named AVFoundation PCM input and ScreenCaptureKit desktop audio.
   Each source owns a bounded queue; device timestamps are not aligned yet. */
private final class ProgramAudioSource: NSObject, AVCaptureAudioDataOutputSampleBufferDelegate, SCStreamOutput, SCStreamDelegate {
    private let condition = NSCondition()
    private let callbacks = DispatchQueue(label: "moobee.audio.source")
    private var samples = [Float]()
    private var failure: String?
    private var closed = false
    private var ready = false
    private var converter: AVAudioConverter?
    private let target = AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: 2)!
    private var session: AVCaptureSession?
    private var output: AVCaptureAudioDataOutput?
    private var desktopDone: DispatchSemaphore?
    private let maxSamples = 192_000

    func start(kind: String, deviceID: String?) throws {
        if kind == "desktop" {
            let done = DispatchSemaphore(value: 0)
            desktopDone = done
            Task.detached { [self] in
                defer { done.signal() }
                do {
                    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
                    guard let display = content.displays.first else { throw micError("No display for desktop audio capture") }
                    if isClosed() { return }
                    let config = SCStreamConfiguration()
                    config.width = 32; config.height = 32
                    config.minimumFrameInterval = CMTime(value: 1, timescale: 1)
                    config.queueDepth = 3
                    config.capturesAudio = true
                    config.sampleRate = 48_000; config.channelCount = 2
                    config.excludesCurrentProcessAudio = true
                    let stream = SCStream(filter: SCContentFilter(display: display, excludingWindows: []), configuration: config, delegate: self)
                    try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: callbacks)
                    try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: callbacks)
                    try await stream.startCapture()
                    markReady()
                    while !isClosed() && !hasFailed() { try await Task.sleep(nanoseconds: 50_000_000) }
                    do { try await stream.stopCapture() }
                    catch { if !isAlreadyStoppedStream(error) { fail(error.localizedDescription) } }
                } catch { fail(error.localizedDescription) }
            }
            condition.lock()
            let deadline = Date(timeIntervalSinceNow: 30)
            while !ready && failure == nil && !closed { if !condition.wait(until: deadline) { break } }
            let result = ready && failure == nil
            let message = failure ?? "Desktop audio startup timed out; check Screen Recording permission"
            condition.unlock()
            if !result { throw micError(message) }
            return
        }
        guard kind == "device" else { throw micError("Unsupported audio input kind") }
        try ProgramMicrophone.authorize()
        let device = deviceID == nil ? AVCaptureDevice.default(for: .audio) :
            AVCaptureDevice.devices(for: .audio).first { $0.uniqueID == deviceID }
        guard let device else { throw micError("Selected audio device is disconnected") }
        let capture = AVCaptureSession()
        let input = try AVCaptureDeviceInput(device: device)
        let sink = AVCaptureAudioDataOutput()
        guard capture.canAddInput(input), capture.canAddOutput(sink) else { throw micError("Audio device cannot be opened") }
        capture.addInput(input)
        capture.addOutput(sink)
        sink.setSampleBufferDelegate(self, queue: callbacks)
        session = capture; output = sink
        capture.startRunning()
        guard capture.isRunning else { throw micError("Audio capture could not start") }
    }
    private func isClosed() -> Bool { condition.lock(); defer { condition.unlock() }; return closed }
    private func hasFailed() -> Bool { condition.lock(); defer { condition.unlock() }; return failure != nil }
    private func markReady() { condition.lock(); ready = true; condition.broadcast(); condition.unlock() }
    private func fail(_ message: String) { condition.lock(); if !closed { failure = message }; condition.broadcast(); condition.unlock() }
    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) { ingest(sampleBuffer) }
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        if type == .audio { ingest(sampleBuffer) }
    }
    func stream(_ stream: SCStream, didStopWithError error: Error) { fail(error.localizedDescription) }
    private func ingest(_ sample: CMSampleBuffer) {
        guard !isClosed(), !hasFailed(), CMSampleBufferIsValid(sample),
              let desc = CMSampleBufferGetFormatDescription(sample),
              let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(desc),
              let source = AVAudioFormat(streamDescription: asbd),
              source.sampleRate >= 8000, source.sampleRate <= 192000,
              source.channelCount > 0, source.channelCount <= 8 else { return }
        let n = CMSampleBufferGetNumSamples(sample)
        guard n > 0, n <= 48_000,
              let pcm = AVAudioPCMBuffer(pcmFormat: source, frameCapacity: AVAudioFrameCount(n)) else { fail("Invalid audio sample geometry"); return }
        pcm.frameLength = AVAudioFrameCount(n)
        guard CMSampleBufferCopyPCMDataIntoAudioBufferList(sample, at: 0, frameCount: Int32(n), into: pcm.mutableAudioBufferList) == noErr else { fail("Cannot copy native PCM"); return }
        if converter == nil || converter?.inputFormat != source {
            converter = AVAudioConverter(from: source, to: target)
        }
        let capacity = AVAudioFrameCount(ceil(Double(n)*48_000/source.sampleRate)+64)
        guard capacity <= 288_064, let converter,
              let converted = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { fail("Audio conversion unavailable"); return }
        var supplied = false
        var error: NSError?
        let status = converter.convert(to: converted, error: &error) { _, flag in
            if supplied { flag.pointee = .noDataNow; return nil }
            supplied = true; flag.pointee = .haveData; return pcm
        }
        guard status != .error, error == nil, let data = converted.floatChannelData else { fail("Native PCM conversion failed"); return }
        let frames = Int(converted.frameLength)
        if frames == 0 { return }
        condition.lock()
        defer { condition.broadcast(); condition.unlock() }
        if closed || failure != nil { return }
        if samples.count + frames*2 > maxSamples { failure = "Audio queue overflow; capture fell behind"; return }
        for i in 0..<frames { samples.append(data[0][i]); samples.append(data[1][i]) }
    }
    func read(_ destination: UnsafeMutablePointer<Float>, frames: Int) throws {
        guard frames > 0, frames <= 4800 else { throw micError("Invalid audio block") }
        condition.lock(); defer { condition.unlock() }
        let deadline = Date(timeIntervalSinceNow: 0.75)
        while samples.count < frames*2 && failure == nil && !closed { if !condition.wait(until: deadline) { break } }
        if let failure { throw micError(failure) }
        guard !closed, samples.count >= frames*2 else { throw micError("Audio input underrun; no samples received") }
        for i in 0..<(frames*2) { destination[i] = samples[i] }
        samples.removeFirst(frames*2)
    }
    func discardQueued() { condition.lock(); samples.removeAll(keepingCapacity: true); condition.unlock() }
    func stop() {
        condition.lock(); closed = true; condition.broadcast(); condition.unlock()
        output?.setSampleBufferDelegate(nil, queue: nil)
        session?.stopRunning()
        if let done = desktopDone { _ = done.wait(timeout: .now()+5); desktopDone = nil }
        callbacks.sync {}
    }
}

@_cdecl("moobee_list_audio_inputs")
public func moobee_list_audio_inputs(_ buffer: UnsafeMutablePointer<CChar>?, _ capacity: Int32) -> Int32 {
    guard let buffer, capacity > 1 else { return -1 }
    let defaultID = AVCaptureDevice.default(for: .audio)?.uniqueID
    let devices = AVCaptureDevice.devices(for: .audio).prefix(128).map {
        ["id": $0.uniqueID, "name": $0.localizedName, "isDefault": $0.uniqueID == defaultID] as [String: Any]
    }
    guard let data = try? JSONSerialization.data(withJSONObject: devices), data.count < Int(capacity) else { return -1 }
    for (i, byte) in data.enumerated() { buffer[i] = CChar(bitPattern: byte) }
    buffer[data.count] = 0
    return 0
}

@_cdecl("moobee_audio_source_create")
public func moobee_audio_source_create(_ kind: UnsafePointer<CChar>?, _ device: UnsafePointer<CChar>?, _ error: UnsafeMutablePointer<CChar>?, _ capacity: Int32) -> UnsafeMutableRawPointer? {
    let source = ProgramAudioSource()
    do {
        guard let kind else { throw micError("Missing audio source kind") }
        try source.start(kind: String(cString: kind), deviceID: device.map { String(cString: $0) })
        return Unmanaged.passRetained(source).toOpaque()
    } catch let cause { source.stop(); micReport(cause,error,capacity); return nil }
}
@_cdecl("moobee_audio_source_read")
public func moobee_audio_source_read(_ handle: UnsafeMutableRawPointer?, _ destination: UnsafeMutablePointer<Float>?, _ frames: Int, _ error: UnsafeMutablePointer<CChar>?, _ capacity: Int32) -> Int32 {
    guard let handle, let destination else { return -1 }
    do { try Unmanaged<ProgramAudioSource>.fromOpaque(handle).takeUnretainedValue().read(destination,frames:frames); return 0 }
    catch let cause { micReport(cause,error,capacity); return -1 }
}
@_cdecl("moobee_audio_source_reset")
public func moobee_audio_source_reset(_ handle: UnsafeMutableRawPointer?) {
    if let handle { Unmanaged<ProgramAudioSource>.fromOpaque(handle).takeUnretainedValue().discardQueued() }
}
@_cdecl("moobee_audio_source_destroy")
public func moobee_audio_source_destroy(_ handle: UnsafeMutableRawPointer?) {
    if let handle { Unmanaged<ProgramAudioSource>.fromOpaque(handle).takeRetainedValue().stop() }
}
