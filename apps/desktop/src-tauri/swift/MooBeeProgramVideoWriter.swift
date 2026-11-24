// First-party composed-frame writer. Rust supplies bounded RGBA canvases and
// a monotonic output frame index; no ScreenCaptureKit single-display shortcut.
import AVFoundation
import AudioToolbox
import CoreMedia
import CoreVideo
import Foundation

private func writerError(_ message: String) -> NSError {
    NSError(domain: "MooBeeProgramVideo", code: -1,
            userInfo: [NSLocalizedDescriptionKey: message])
}

private func reportError(_ error: Error, _ output: UnsafeMutablePointer<CChar>?, _ capacity: Int32) {
    guard let output, capacity > 0 else { return }
    let bytes = Array(error.localizedDescription.utf8.prefix(Int(capacity) - 1))
    for (i, byte) in bytes.enumerated() { output[i] = CChar(bitPattern: byte) }
    output[bytes.count] = 0
}

// All calls are serialized by the Rust capture worker. The pixel buffers are
// retained by the adaptor after append; no pointer to Rust memory escapes.
private final class ProgramVideoWriter {
    let writer: AVAssetWriter
    let input: AVAssetWriterInput
    let adaptor: AVAssetWriterInputPixelBufferAdaptor
    let audioInput: AVAssetWriterInput?
    let audioFormat: AVAudioFormat?
    var audioFrames: Int64 = 0
    let width: Int
    let height: Int
    let fps: Int32
    var count: Int64 = 0
    var finishing = false

    init(path: String, width: Int, height: Int, fps: Int32, microphone: Bool) throws {
        guard (width == 640 && height == 360 && fps == 10) ||
              (width == 1280 && height == 720 && fps == 15) ||
              (width == 1920 && height == 1080 && fps == 30) else {
            throw writerError("Unsupported composed H.264 test profile")
        }
        self.width = width
        self.height = height
        self.fps = fps
        let asset = try AVAssetWriter(outputURL: URL(fileURLWithPath: path), fileType: .mp4)
        let track = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: width,
            AVVideoHeightKey: height,
            AVVideoCompressionPropertiesKey: [
                AVVideoAverageBitRateKey: width == 1920 ? 6_000_000 : 3_000_000,
                AVVideoExpectedSourceFrameRateKey: Int(fps),
                AVVideoMaxKeyFrameIntervalKey: Int(fps) * 2,
                AVVideoAllowFrameReorderingKey: false
            ]
        ])
        track.expectsMediaDataInRealTime = true
        self.writer = asset
        self.input = track
        self.adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: track,
            sourcePixelBufferAttributes: [
                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
                kCVPixelBufferWidthKey as String: width,
                kCVPixelBufferHeightKey as String: height
            ])
        if microphone {
            guard let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 48_000, channels: 2, interleaved: true) else {
                throw writerError("Cannot create stereo PCM format")
            }
            audioFormat = format
            let audio = AVAssetWriterInput(mediaType: .audio, outputSettings: [
                AVFormatIDKey: kAudioFormatMPEG4AAC,
                AVSampleRateKey: 48_000, AVNumberOfChannelsKey: 2, AVEncoderBitRateKey: 128_000
            ], sourceFormatHint: format.formatDescription)
            audio.expectsMediaDataInRealTime = true
            guard asset.canAdd(audio) else { throw writerError("AAC encoder unavailable") }
            asset.add(audio)
            audioInput = audio
        } else {
            audioFormat = nil
            audioInput = nil
        }
        guard asset.canAdd(track) else { throw writerError("H.264 encoder unavailable") }
        asset.add(track)
        guard asset.startWriting() else { throw asset.error ?? writerError("Could not start H.264 writer") }
        asset.startSession(atSourceTime: .zero)
    }

    func append(_ rgba: UnsafePointer<UInt8>, length: Int) throws {
        guard !finishing, count < 120, length == width * height * 4 else {
            throw writerError("Invalid composed frame or frame budget exceeded")
        }
        // Bounded backpressure, not an unbounded queued recording.
        let deadline = ProcessInfo.processInfo.systemUptime + 0.5
        while !input.isReadyForMoreMediaData && writer.status == .writing &&
              ProcessInfo.processInfo.systemUptime < deadline {
            Thread.sleep(forTimeInterval: 0.005)
        }
        guard writer.status == .writing, input.isReadyForMoreMediaData,
              let pool = adaptor.pixelBufferPool else {
            throw writer.error ?? writerError("H.264 writer stalled")
        }
        var pixel: CVPixelBuffer?
        guard CVPixelBufferPoolCreatePixelBuffer(kCFAllocatorDefault, pool, &pixel) == kCVReturnSuccess,
              let pixel else { throw writerError("Could not allocate H.264 pixel buffer") }
        guard CVPixelBufferLockBaseAddress(pixel, []) == kCVReturnSuccess else {
            throw writerError("Could not lock H.264 pixel buffer")
        }
        guard let base = CVPixelBufferGetBaseAddress(pixel) else {
            CVPixelBufferUnlockBaseAddress(pixel, [])
            throw writerError("Missing H.264 pixel buffer memory")
        }
        let dst = base.assumingMemoryBound(to: UInt8.self)
        let stride = CVPixelBufferGetBytesPerRow(pixel)
        for y in 0..<height {
            for x in 0..<width {
                let srcOffset = (y * width + x) * 4
                let dstOffset = y * stride + x * 4
                dst[dstOffset] = rgba[srcOffset + 2]
                dst[dstOffset + 1] = rgba[srcOffset + 1]
                dst[dstOffset + 2] = rgba[srcOffset]
                dst[dstOffset + 3] = 255
            }
        }
        CVPixelBufferUnlockBaseAddress(pixel, [])
        guard adaptor.append(pixel, withPresentationTime: CMTime(value: count, timescale: fps)) else {
            throw writer.error ?? writerError("H.264 encoder rejected a composed frame")
        }
        count += 1
    }

    func appendAudio(_ samples: UnsafePointer<Float>, frames: Int) throws {
        guard !finishing, let track = audioInput, let format = audioFormat,
              frames > 0, frames <= 4800,
              audioFrames + Int64(frames) <= count * 48_000 / Int64(fps) else {
            throw writerError("Invalid or out-of-order audio block")
        }
        let deadline = ProcessInfo.processInfo.systemUptime + 0.5
        while !track.isReadyForMoreMediaData && writer.status == .writing &&
              ProcessInfo.processInfo.systemUptime < deadline { Thread.sleep(forTimeInterval: 0.005) }
        guard track.isReadyForMoreMediaData, writer.status == .writing else {
            throw writer.error ?? writerError("AAC writer stalled")
        }
        let bytes = frames * 2 * MemoryLayout<Float>.size
        var block: CMBlockBuffer?
        guard CMBlockBufferCreateWithMemoryBlock(allocator: kCFAllocatorDefault, memoryBlock: nil,
            blockLength: bytes, blockAllocator: kCFAllocatorDefault, customBlockSource: nil,
            offsetToData: 0, dataLength: bytes, flags: 0, blockBufferOut: &block) == noErr,
            let block, CMBlockBufferReplaceDataBytes(with: UnsafeRawPointer(samples), blockBuffer: block,
                offsetIntoDestination: 0, dataLength: bytes) == noErr else {
            throw writerError("Unable to allocate PCM sample buffer")
        }
        var timing = CMSampleTimingInfo(duration: CMTime(value: 1, timescale: 48_000),
            presentationTimeStamp: CMTime(value: audioFrames, timescale: 48_000), decodeTimeStamp: .invalid)
        var size = 2 * MemoryLayout<Float>.size
        var sample: CMSampleBuffer?
        guard CMSampleBufferCreateReady(allocator: kCFAllocatorDefault, dataBuffer: block,
            formatDescription: format.formatDescription, sampleCount: frames,
            sampleTimingEntryCount: 1, sampleTimingArray: &timing, sampleSizeEntryCount: 1,
            sampleSizeArray: &size, sampleBufferOut: &sample) == noErr,
            let sample, track.append(sample) else {
            throw writer.error ?? writerError("AAC encoder rejected PCM")
        }
        audioFrames += Int64(frames)
    }

    func finish() -> Int32 {
        guard !finishing, count >= 2, writer.status == .writing,
              audioInput == nil || audioFrames == count * 48_000 / Int64(fps) else { return -1 }
        finishing = true
        writer.endSession(atSourceTime: CMTime(value: count, timescale: fps))
        input.markAsFinished()
        audioInput?.markAsFinished()
        let done = DispatchSemaphore(value: 0)
        let retainedWriter = writer
        retainedWriter.finishWriting { _ = retainedWriter.status; done.signal() }
        // A timed-out output must stay hidden. Do not race the active writer
        // by renaming or deleting its file from Rust.
        guard done.wait(timeout: .now() + 15) == .success else { return -2 }
        return writer.status == .completed ? 0 : -1
    }

    deinit {
        if !finishing && writer.status == .writing { writer.cancelWriting() }
    }
}

@_cdecl("moobee_program_writer_create")
public func moobee_program_writer_create(
    _ path: UnsafePointer<CChar>?, _ width: Int32, _ height: Int32, _ fps: Int32, _ microphone: Int32,
    _ error: UnsafeMutablePointer<CChar>?, _ capacity: Int32
) -> UnsafeMutableRawPointer? {
    do {
        guard let path, let value = String(validatingUTF8: path) else {
            throw writerError("Invalid composed MP4 path")
        }
        let sink = try ProgramVideoWriter(path: value, width: Int(width), height: Int(height), fps: fps, microphone: microphone == 1)
        return Unmanaged.passRetained(sink).toOpaque()
    } catch let cause { reportError(cause, error, capacity); return nil }
}

@_cdecl("moobee_program_writer_append")
public func moobee_program_writer_append(
    _ handle: UnsafeMutableRawPointer?, _ rgba: UnsafePointer<UInt8>?, _ length: Int,
    _ error: UnsafeMutablePointer<CChar>?, _ capacity: Int32
) -> Int32 {
    do {
        guard let handle, let rgba else { throw writerError("Invalid composed frame writer handle") }
        try Unmanaged<ProgramVideoWriter>.fromOpaque(handle).takeUnretainedValue().append(rgba, length: length)
        return 0
    } catch let cause { reportError(cause, error, capacity); return -1 }
}

@_cdecl("moobee_program_writer_append_audio")
public func moobee_program_writer_append_audio(
    _ handle: UnsafeMutableRawPointer?, _ pcm: UnsafePointer<Float>?, _ frames: Int,
    _ error: UnsafeMutablePointer<CChar>?, _ capacity: Int32
) -> Int32 {
    do {
        guard let handle, let pcm else { throw writerError("Invalid PCM writer input") }
        try Unmanaged<ProgramVideoWriter>.fromOpaque(handle).takeUnretainedValue().appendAudio(pcm, frames: frames)
        return 0
    } catch let cause { reportError(cause, error, capacity); return -1 }
}

@_cdecl("moobee_program_writer_finish")
public func moobee_program_writer_finish(
    _ handle: UnsafeMutableRawPointer?, _ error: UnsafeMutablePointer<CChar>?, _ capacity: Int32
) -> Int32 {
    guard let handle else { return -1 }
    let sink = Unmanaged<ProgramVideoWriter>.fromOpaque(handle).takeUnretainedValue()
    let result = sink.finish()
    if result != 0 {
        reportError(sink.writer.error ?? writerError(result == -2 ?
            "H.264 finalization timed out; incomplete file retained privately" :
            "H.264 writer failed to finalize"), error, capacity)
    }
    return result
}

@_cdecl("moobee_program_writer_destroy")
public func moobee_program_writer_destroy(_ handle: UnsafeMutableRawPointer?) {
    if let handle { Unmanaged<ProgramVideoWriter>.fromOpaque(handle).release() }
}
