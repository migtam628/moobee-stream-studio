// macOS 14 ScreenCaptureKit -> first-party AVFoundation H.264 MP4 fallback.
// SCRecordingOutput itself is macOS 15+. Sonoma instead delivers uncompressed
// SCStream sample buffers, which we encode/mux in process with AVAssetWriter.
// This records a single display, NOT MooBee's composite Program scene.
// Until actual hardware tests complete, this is an experimental capture path.

import AVFoundation
import CoreMedia
import CoreVideo
import Foundation
import ScreenCaptureKit

@available(macOS 14.0, *)
private final class SonomaMP4Sink: NSObject, SCStreamOutput, SCStreamDelegate {
    let callbackQueue = DispatchQueue(label: "com.moobee.sonoma.mp4.writer", qos: .userInitiated)
    private let writer: AVAssetWriter
    private let video: AVAssetWriterInput
    private let adaptor: AVAssetWriterInputPixelBufferAdaptor
    private let audio: AVAssetWriterInput?
    private var startedAt: CMTime?
    private var lastFrameTime: CMTime?
    private var frameCount = 0
    private var captureError: Error?

    init(output: URL, width: Int, height: Int, systemAudio: Bool) throws {
        writer = try AVAssetWriter(outputURL: output, fileType: .mp4)
        let settings: [String: Any] = [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: width,
            AVVideoHeightKey: height,
            AVVideoCompressionPropertiesKey: [
                AVVideoAverageBitRateKey: 6_000_000,
                AVVideoExpectedSourceFrameRateKey: 30,
                AVVideoMaxKeyFrameIntervalKey: 60
            ]
        ]
        video = AVAssetWriterInput(mediaType: .video, outputSettings: settings)
        video.expectsMediaDataInRealTime = true
        adaptor = AVAssetWriterInputPixelBufferAdaptor(
            assetWriterInput: video,
            sourcePixelBufferAttributes: [
                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
                kCVPixelBufferWidthKey as String: width,
                kCVPixelBufferHeightKey as String: height
            ]
        )
        if systemAudio {
            let settings: [String: Any] = [
                AVFormatIDKey: kAudioFormatMPEG4AAC,
                AVSampleRateKey: 48_000,
                AVNumberOfChannelsKey: 2,
                AVEncoderBitRateKey: 128_000
            ]
            let input = AVAssetWriterInput(mediaType: .audio, outputSettings: settings)
            input.expectsMediaDataInRealTime = true
            audio = input
        } else {
            audio = nil
        }
        super.init()
        guard writer.canAdd(video) else {
            throw NSError(domain: "MooBeeSonomaRecorder", code: -1,
                          userInfo: [NSLocalizedDescriptionKey: "Cannot add H.264 video to MP4 writer"])
        }
        writer.add(video)
        if let audio {
            guard writer.canAdd(audio) else {
                throw NSError(domain: "MooBeeSonomaRecorder", code: -2,
                              userInfo: [NSLocalizedDescriptionKey: "Cannot add system audio to MP4 writer"])
            }
            writer.add(audio)
        }
    }

    // ScreenCaptureKit submits both streams to callbackQueue. All writer state
    // is confined to that serial queue; stop/finalization drains it before use.
    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer,
                of type: SCStreamOutputType) {
        guard CMSampleBufferIsValid(buffer), CMSampleBufferDataIsReady(buffer) else { return }
        let time = CMSampleBufferGetPresentationTimeStamp(buffer)
        guard time.isValid, time.isNumeric else { return }
        if type == .screen {
            guard let pixelBuffer = CMSampleBufferGetImageBuffer(buffer) else { return }
            if startedAt == nil {
                guard writer.startWriting() else {
                    captureError = writer.error ?? NSError(
                        domain: "MooBeeSonomaRecorder", code: -3,
                        userInfo: [NSLocalizedDescriptionKey: "Could not start H.264 encoder"])
                    return
                }
                writer.startSession(atSourceTime: time)
                startedAt = time
            }
            guard writer.status == .writing else { return }
            if video.isReadyForMoreMediaData {
                if adaptor.append(pixelBuffer, withPresentationTime: time) {
                    frameCount += 1
                    lastFrameTime = time
                } else {
                    captureError = writer.error ?? NSError(
                        domain: "MooBeeSonomaRecorder", code: -4,
                        userInfo: [NSLocalizedDescriptionKey: "H.264 encoder rejected a screen frame"])
                }
            }
        } else if type == .audio, let audio, let origin = startedAt,
                  CMTimeCompare(time, origin) >= 0, writer.status == .writing,
                  audio.isReadyForMoreMediaData {
            if !audio.append(buffer) {
                captureError = writer.error ?? NSError(
                    domain: "MooBeeSonomaRecorder", code: -5,
                    userInfo: [NSLocalizedDescriptionKey: "AAC encoder rejected system audio"])
            }
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        callbackQueue.async { [self] in
            captureError = error
        }
    }

    func finish() async throws {
        // SCStream.stopCapture() has returned, so drain its callback queue.
        // Do not make a successful MP4 visible without the writer finishing.
        let state: (Int, Error?, AVAssetWriter.Status) = callbackQueue.sync {
            if writer.status == .writing {
                if let lastFrameTime {
                    writer.endSession(atSourceTime: lastFrameTime)
                }
                video.markAsFinished()
                audio?.markAsFinished()
            }
            return (frameCount, captureError, writer.status)
        }
        if let error = state.1 { throw error }
        guard state.0 >= 2, state.2 == .writing else {
            throw NSError(domain: "MooBeeSonomaRecorder", code: -6,
                          userInfo: [NSLocalizedDescriptionKey:
                            "No complete H.264 video frames were written"])
        }
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            writer.finishWriting { continuation.resume() }
        }
        guard writer.status == .completed else {
            throw writer.error ?? NSError(
                domain: "MooBeeSonomaRecorder", code: -7,
                userInfo: [NSLocalizedDescriptionKey: "MP4 writer did not finalize"])
        }
    }
}

/// The Swift task uses the same stop flag and Rust capture mutex as macOS 15.
/// macOS 14 supports ScreenCaptureKit system audio, but its separate microphone
/// stream output API is unavailable. A microphone request fails explicitly.
@available(macOS 14.0, *)
@MainActor
func recordSonomaMP4(
    url: URL, displayID: UInt32, seconds: Int, systemAudio: Bool,
    microphone: Bool, shouldStop: () -> Bool
) async throws {
    guard !microphone else {
        throw NSError(domain: "MooBeeSonomaRecorder", code: -8,
                      userInfo: [NSLocalizedDescriptionKey:
                        "Microphone-in-MP4 requires macOS 15+; disable microphone for Sonoma capture"])
    }
    let available = try await SCShareableContent.excludingDesktopWindows(
        false, onScreenWindowsOnly: false)
    guard let display = available.displays.first(where: { $0.displayID == displayID }) else {
        throw NSError(domain: "MooBeeSonomaRecorder", code: -9,
                      userInfo: [NSLocalizedDescriptionKey:
                        "Selected display is unavailable. Refresh capture devices."])
    }

    let width = max(16, Int(display.width))
    let height = max(16, Int(display.height))
    let scale = min(1.0, 1920.0 / Double(width), 1080.0 / Double(height))
    let outputWidth = max(16, Int(Double(width) * scale) / 2 * 2)
    let outputHeight = max(16, Int(Double(height) * scale) / 2 * 2)
    let config = SCStreamConfiguration()
    config.width = outputWidth
    config.height = outputHeight
    config.minimumFrameInterval = CMTime(value: 1, timescale: 30)
    config.queueDepth = 5
    config.pixelFormat = kCVPixelFormatType_32BGRA
    config.capturesAudio = systemAudio
    if systemAudio {
        config.sampleRate = 48_000
        config.channelCount = 2
    }
    let sink = try SonomaMP4Sink(
        output: url, width: outputWidth, height: outputHeight, systemAudio: systemAudio)
    let filter = SCContentFilter(display: display, excludingWindows: [])
    let stream = SCStream(filter: filter, configuration: config, delegate: sink)
    try stream.addStreamOutput(sink, type: .screen, sampleHandlerQueue: sink.callbackQueue)
    if systemAudio {
        try stream.addStreamOutput(sink, type: .audio, sampleHandlerQueue: sink.callbackQueue)
    }
    try await stream.startCapture()
    do {
        let deadline = Date().addingTimeInterval(TimeInterval(seconds))
        while Date() < deadline && !shouldStop() {
            try await Task.sleep(nanoseconds: 250_000_000)
        }
    } catch {
        try? await stream.stopCapture()
        throw error
    }
    try await stream.stopCapture()
    try await sink.finish()
    let size = try FileManager.default.attributesOfItem(atPath: url.path)[.size] as? NSNumber
    guard (size?.intValue ?? 0) >= 4096 else {
        throw NSError(domain: "MooBeeSonomaRecorder", code: -10,
                      userInfo: [NSLocalizedDescriptionKey:
                        "AVAssetWriter finished but produced no playable-sized MP4"])
    }
}
