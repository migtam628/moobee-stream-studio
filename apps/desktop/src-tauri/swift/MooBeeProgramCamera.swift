import AVFoundation
import CoreMedia
import CoreVideo
import Foundation

private func cameraError(_ message: String) -> NSError {
    NSError(domain: "MooBeeProgramCamera", code: -1,
            userInfo: [NSLocalizedDescriptionKey: message])
}
private func cameraReport(_ cause: Error, _ output: UnsafeMutablePointer<CChar>?, _ capacity: Int32) {
    guard let output, capacity > 0 else { return }
    let bytes = Array(cause.localizedDescription.utf8.prefix(Int(capacity) - 1))
    for (i, byte) in bytes.enumerated() { output[i] = CChar(bitPattern: byte) }
    output[bytes.count] = 0
}

// Capture worker owns the session; delegate queue retains only the most
// recent pixel buffer. No audio device or external process is requested.
private final class ProgramCamera: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    let session = AVCaptureSession()
    let output = AVCaptureVideoDataOutput()
    let queue = DispatchQueue(label: "com.moobee.program-camera.frames")
    let lock = NSLock()
    private var latest: CVPixelBuffer?
    private var sequence: UInt64 = 0

    func start(deviceID: String?) throws {
        if AVCaptureDevice.authorizationStatus(for: .video) == .notDetermined {
            let done = DispatchSemaphore(value: 0)
            AVCaptureDevice.requestAccess(for: .video) { _ in done.signal() }
            guard done.wait(timeout: .now() + 30) == .success else {
                throw cameraError("Camera permission request timed out; retry after responding")
            }
        }
        guard AVCaptureDevice.authorizationStatus(for: .video) == .authorized else {
            throw cameraError("Camera permission denied; enable MooBee in macOS Privacy & Security > Camera")
        }
        let selected = deviceID == nil ? AVCaptureDevice.default(for: .video) :
            AVCaptureDevice.devices(for: .video).first { $0.uniqueID == deviceID }
        guard let device = selected else {
            throw cameraError("Selected video device is disconnected or unavailable; refresh video inputs")
        }
        let input = try AVCaptureDeviceInput(device: device)
        session.beginConfiguration()
        if deviceID != nil && session.canSetSessionPreset(.hd1280x720) { session.sessionPreset = .hd1280x720 }
        else if session.canSetSessionPreset(.vga640x480) { session.sessionPreset = .vga640x480 }
        else if session.canSetSessionPreset(.medium) { session.sessionPreset = .medium }
        guard session.canAddInput(input), session.canAddOutput(output) else {
            session.commitConfiguration()
            throw cameraError("Default webcam cannot be configured")
        }
        session.addInput(input)
        output.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA]
        output.alwaysDiscardsLateVideoFrames = true
        output.setSampleBufferDelegate(self, queue: queue)
        session.addOutput(output)
        session.commitConfiguration()
        session.startRunning()
        guard session.isRunning else { throw cameraError("Webcam session failed to start") }
    }
    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer,
                       from connection: AVCaptureConnection) {
        guard let pixel = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        lock.lock()
        latest = pixel
        sequence &+= 1
        lock.unlock()
    }

    // Copies a new BGRA frame into a bounded caller-owned RGBA buffer.
    // 0 means no new sample, 1 means success, -1 means invalid geometry.
    func copyFrame(_ destination: UnsafeMutablePointer<UInt8>, capacity: Int,
                   after: UInt64, width: UnsafeMutablePointer<Int32>,
                   height: UnsafeMutablePointer<Int32>, number: UnsafeMutablePointer<UInt64>) -> Int32 {
        lock.lock()
        let pixel = latest
        let current = sequence
        lock.unlock()
        guard current != after, let pixel else { return 0 }
        let w = CVPixelBufferGetWidth(pixel), h = CVPixelBufferGetHeight(pixel)
        guard w > 0, h > 0, w <= 1280, h <= 720, w * h * 4 <= capacity,
              CVPixelBufferGetPixelFormatType(pixel) == kCVPixelFormatType_32BGRA,
              CVPixelBufferLockBaseAddress(pixel, .readOnly) == kCVReturnSuccess else { return -1 }
        defer { CVPixelBufferUnlockBaseAddress(pixel, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(pixel) else { return -1 }
        let source = base.assumingMemoryBound(to: UInt8.self)
        let stride = CVPixelBufferGetBytesPerRow(pixel)
        guard stride >= w * 4 else { return -1 }
        for y in 0..<h {
            for x in 0..<w {
                let src = y * stride + x * 4, dst = (y * w + x) * 4
                destination[dst] = source[src + 2]
                destination[dst + 1] = source[src + 1]
                destination[dst + 2] = source[src]
                destination[dst + 3] = 255
            }
        }
        width.pointee = Int32(w)
        height.pointee = Int32(h)
        number.pointee = current
        return 1
    }
    func stop() {
        output.setSampleBufferDelegate(nil, queue: nil)
        if session.isRunning { session.stopRunning() }
        // Drain callbacks before releasing the delegate and retained frame.
        queue.sync {}
        lock.lock()
        latest = nil
        lock.unlock()
    }
}

@_cdecl("moobee_program_camera_create")
public func moobee_program_camera_create(_ deviceID: UnsafePointer<CChar>?, _ error: UnsafeMutablePointer<CChar>?,
                                         _ capacity: Int32) -> UnsafeMutableRawPointer? {
    let camera = ProgramCamera()
    do {
        let selected = deviceID.map { String(cString: $0) }
        try camera.start(deviceID: selected)
        return Unmanaged.passRetained(camera).toOpaque()
    } catch let cause {
        camera.stop()
        cameraReport(cause, error, capacity)
        return nil
    }
}

@_cdecl("moobee_program_camera_copy")
public func moobee_program_camera_copy(_ handle: UnsafeMutableRawPointer?,
                                       _ destination: UnsafeMutablePointer<UInt8>?, _ capacity: Int,
                                       _ after: UInt64, _ width: UnsafeMutablePointer<Int32>?,
                                       _ height: UnsafeMutablePointer<Int32>?,
                                       _ sequence: UnsafeMutablePointer<UInt64>?) -> Int32 {
    guard let handle, let destination, let width, let height, let sequence else { return -1 }
    return Unmanaged<ProgramCamera>.fromOpaque(handle).takeUnretainedValue()
        .copyFrame(destination, capacity: capacity, after: after, width: width, height: height, number: sequence)
}

@_cdecl("moobee_program_camera_destroy")
public func moobee_program_camera_destroy(_ handle: UnsafeMutableRawPointer?) {
    guard let handle else { return }
    let camera = Unmanaged<ProgramCamera>.fromOpaque(handle).takeRetainedValue()
    camera.stop()
}

// Discovery does not start any camera or request capture permission.
// Include external UVC devices by their stable AVFoundation unique IDs.
// Device names are descriptive only; no assumption that every USB camera
// is an HDMI capture card.
@_cdecl("moobee_list_video_inputs")
public func moobee_list_video_inputs(_ buffer: UnsafeMutablePointer<CChar>?, _ capacity: Int32) -> Int32 {
    guard let buffer, capacity > 1 else { return -1 }
    let devices = AVCaptureDevice.devices(for: .video)
    let defaultID = AVCaptureDevice.default(for: .video)?.uniqueID
    let entries: [[String: Any]] = devices.prefix(128).map { device in
        ["id": device.uniqueID, "name": device.localizedName,
         "isDefault": device.uniqueID == defaultID]
    }
    guard let data = try? JSONSerialization.data(withJSONObject: entries),
          data.count < Int(capacity) else { return -1 }
    for (index, byte) in data.enumerated() { buffer[index] = CChar(bitPattern: byte) }
    buffer[data.count] = 0
    return 0
}
