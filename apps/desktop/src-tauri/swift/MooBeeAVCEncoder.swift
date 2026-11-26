// VideoToolbox access-unit bridge. No disk writer, socket or live service.
import Foundation
import CoreMedia
import CoreVideo
import VideoToolbox

private func avcError(_ output: UnsafeMutablePointer<CChar>?, _ capacity: Int32, _ message: String) {
    guard let output, capacity > 0 else { return }
    let bytes = Array(message.utf8.prefix(Int(capacity)-1))
    for (i,b) in bytes.enumerated() { output[i] = CChar(bitPattern:b) }
    output[bytes.count] = 0
}
private final class EncodedFrame {
    let lock = NSLock()
    var sample: CMSampleBuffer?
    var status: OSStatus = -1
    let done = DispatchSemaphore(value:0)
}
private let outputCallback: VTCompressionOutputCallback = { ref, _, status, _, sample in
    guard let ref else { return }
    let frame = Unmanaged<EncodedFrame>.fromOpaque(ref).takeUnretainedValue()
    frame.lock.lock()
    frame.status = status
    frame.sample = sample
    frame.lock.unlock()
    frame.done.signal()
}
private final class AVCEncoder {
    let frame = EncodedFrame()
    var session: VTCompressionSession?
    let width:Int
    let height:Int
    let fps:Int32
    var index:Int64 = 0
    var failed = false
    // Serialized calls from the owning Rust worker only.
    init?(_ width:Int32,_ height:Int32,_ fps:Int32,_ kbps:Int32) {
        guard (width == 640 && height == 360 && fps == 10) ||
              (width == 1280 && height == 720 && fps == 15) ||
              (width == 1920 && height == 1080 && fps == 30),
              [Int32(1500),3000,6000].contains(kbps) else { return nil }
        self.width=Int(width); self.height=Int(height); self.fps=fps
        var created: VTCompressionSession?
        let status = VTCompressionSessionCreate(allocator:kCFAllocatorDefault,
            width:width,height:height,codecType:kCMVideoCodecType_H264,
            encoderSpecification:nil,imageBufferAttributes:nil,
            compressedDataAllocator:nil,outputCallback:outputCallback,
            refcon:Unmanaged.passUnretained(frame).toOpaque(),compressionSessionOut:&created)
        guard status == noErr, let created else { return nil }
        session=created
        let props: [(CFString, CFTypeRef)] = [
            (kVTCompressionPropertyKey_RealTime,kCFBooleanTrue),
            (kVTCompressionPropertyKey_AllowFrameReordering,kCFBooleanFalse),
            (kVTCompressionPropertyKey_ProfileLevel,kVTProfileLevel_H264_Main_AutoLevel),
            (kVTCompressionPropertyKey_AverageBitRate,NSNumber(value:kbps*1000)),
            (kVTCompressionPropertyKey_ExpectedFrameRate,NSNumber(value:fps)),
            (kVTCompressionPropertyKey_MaxKeyFrameInterval,NSNumber(value:fps*2))
        ]
        for (key,value) in props {
            guard VTSessionSetProperty(created,key:key,value:value) == noErr else {
                VTCompressionSessionInvalidate(created);session=nil;return nil
            }
        }
        guard VTCompressionSessionPrepareToEncodeFrames(created) == noErr else {
            VTCompressionSessionInvalidate(created);session=nil;return nil
        }
    }
    deinit { if let session { VTCompressionSessionInvalidate(session) } }
    func encode(_ rgba:UnsafePointer<UInt8>,_ length:Int,_ forceKey:Bool,
                _ out:UnsafeMutablePointer<UInt8>,_ capacity:Int,
                _ sps:UnsafeMutablePointer<UInt8>,_ pps:UnsafeMutablePointer<UInt8>,_ configCapacity:Int,
                _ spsLength:UnsafeMutablePointer<Int>,_ ppsLength:UnsafeMutablePointer<Int>,
                _ key:UnsafeMutablePointer<Int32>) -> Int {
        guard !failed,let session,length==width*height*4,index<Int64(fps)*3600,
              capacity>0,configCapacity>0 else {return -1}
        var pixel:CVPixelBuffer?
        guard CVPixelBufferCreate(kCFAllocatorDefault,width,height,kCVPixelFormatType_32BGRA,
              [kCVPixelBufferIOSurfacePropertiesKey:[:]] as CFDictionary,&pixel)==kCVReturnSuccess,
              let pixel,CVPixelBufferLockBaseAddress(pixel,[])==kCVReturnSuccess else{return -1}
        guard let base=CVPixelBufferGetBaseAddress(pixel) else {
            CVPixelBufferUnlockBaseAddress(pixel,[]);return -1
        }
        let dst=base.assumingMemoryBound(to:UInt8.self)
        let stride=CVPixelBufferGetBytesPerRow(pixel)
        for y in 0..<height {for x in 0..<width {
            let i=(y*width+x)*4,j=y*stride+x*4
            dst[j]=rgba[i+2];dst[j+1]=rgba[i+1];dst[j+2]=rgba[i];dst[j+3]=255
        }}
        CVPixelBufferUnlockBaseAddress(pixel,[])
        frame.lock.lock();frame.sample=nil;frame.status = -1;frame.lock.unlock()
        let options = (forceKey || index==0) ? [kVTEncodeFrameOptionKey_ForceKeyFrame:kCFBooleanTrue] as CFDictionary : nil
        let rc=VTCompressionSessionEncodeFrame(session,imageBuffer:pixel,
            presentationTimeStamp:CMTime(value:index,timescale:fps),
            duration:CMTime(value:1,timescale:fps),frameProperties:options,
            sourceFrameRefcon:nil,infoFlagsOut:nil)
        if rc != noErr {failed=true;return -1}
        // Flush each submitted frame. The diagnostic bridge is intentionally
        // synchronous; native driver stalls still need a separate watchdog.
        if VTCompressionSessionCompleteFrames(session,untilPresentationTimeStamp:.invalid) != noErr {
            failed=true;return -1
        }
        guard frame.done.wait(timeout: .now() + 2) == .success else {failed=true;return -1}
        frame.lock.lock();defer{frame.lock.unlock()}
        guard frame.status==noErr,let sample=frame.sample,
              let block=CMSampleBufferGetDataBuffer(sample),
              let format=CMSampleBufferGetFormatDescription(sample) else {failed=true;return -1}
        let size=CMBlockBufferGetDataLength(block)
        guard size>0,size<=capacity,CMBlockBufferCopyDataBytes(block,atOffset:0,dataLength:size,destination:out)==noErr else {failed=true;return -1}
        // Output must use four-byte AVCC NAL lengths, as expected by Rust.
        var spsPtr:UnsafePointer<UInt8>?,ppsPtr:UnsafePointer<UInt8>?
        var sn=0,pn=0,count=0,header:Int32=0
        guard CMVideoFormatDescriptionGetH264ParameterSetAtIndex(format,parameterSetIndex:0,
              parameterSetPointerOut:&spsPtr,parameterSetSizeOut:&sn,parameterSetCountOut:&count,nalUnitHeaderLengthOut:&header)==noErr,
              count==2,header==4,let spsPtr,sn>0,sn<=configCapacity,
              CMVideoFormatDescriptionGetH264ParameterSetAtIndex(format,parameterSetIndex:1,
              parameterSetPointerOut:&ppsPtr,parameterSetSizeOut:&pn,parameterSetCountOut:nil,nalUnitHeaderLengthOut:nil)==noErr,
              let ppsPtr,pn>0,pn<=configCapacity else {failed=true;return -1}
        sps.update(from:spsPtr,count:sn);pps.update(from:ppsPtr,count:pn)
        spsLength.pointee=sn;ppsLength.pointee=pn
        // Inspect actual NAL units, not a requested keyframe flag.
        var offset=0,idr=false
        while offset<size {
            guard size-offset>=4 else {failed=true;return -1}
            let n=Int(out[offset])<<24 | Int(out[offset+1])<<16 | Int(out[offset+2])<<8 | Int(out[offset+3])
            offset+=4
            guard n>0,n<=size-offset else {failed=true;return -1}
            idr = idr || (out[offset]&0x1f)==5
            offset+=n
        }
        if (index==0 || forceKey) && !idr {failed=true;return -1}
        key.pointee=idr ? 1:0
        index+=1
        return size
    }
}
@_cdecl("moobee_avc_create")
public func moobee_avc_create(_ width:Int32,_ height:Int32,_ fps:Int32,_ kbps:Int32,
    _ error:UnsafeMutablePointer<CChar>?,_ capacity:Int32)->UnsafeMutableRawPointer? {
    guard let encoder=AVCEncoder(width,height,fps,kbps) else {
        avcError(error,capacity,"VideoToolbox encoder initialization failed");return nil
    }
    return Unmanaged.passRetained(encoder).toOpaque()
}
@_cdecl("moobee_avc_encode")
public func moobee_avc_encode(_ handle:UnsafeMutableRawPointer?,_ rgba:UnsafePointer<UInt8>?,_ length:Int,
    _ forceKey:Int32,_ output:UnsafeMutablePointer<UInt8>?,_ capacity:Int,
    _ sps:UnsafeMutablePointer<UInt8>?,_ pps:UnsafeMutablePointer<UInt8>?,_ configCapacity:Int,
    _ spsLength:UnsafeMutablePointer<Int>?,_ ppsLength:UnsafeMutablePointer<Int>?,
    _ key:UnsafeMutablePointer<Int32>?,_ error:UnsafeMutablePointer<CChar>?,_ errorCapacity:Int32)->Int {
    guard let handle,let rgba,let output,let sps,let pps,let spsLength,let ppsLength,let key else{return -1}
    let n=Unmanaged<AVCEncoder>.fromOpaque(handle).takeUnretainedValue().encode(rgba,length,forceKey != 0,
        output,capacity,sps,pps,configCapacity,spsLength,ppsLength,key)
    if n<0 {avcError(error,errorCapacity,"VideoToolbox access unit missing, stalled or unsupported")}
    return n
}
@_cdecl("moobee_avc_destroy")
public func moobee_avc_destroy(_ handle:UnsafeMutableRawPointer?) {
    if let handle {Unmanaged<AVCEncoder>.fromOpaque(handle).release()}
}
