// Native AAC-LC access units from bounded 48 kHz stereo PCM.
// No AVAssetWriter, file, socket, or external encoder.
import Foundation
import AVFoundation
import AudioToolbox

private func aacError(_ out:UnsafeMutablePointer<CChar>?,_ cap:Int32,_ message:String){
    guard let out,cap>0 else{return}
    let bytes=Array(message.utf8.prefix(Int(cap)-1))
    for (i,b) in bytes.enumerated(){out[i]=CChar(bitPattern:b)}
    out[bytes.count]=0
}
private final class AACEncoder{
    let converter:AVAudioConverter
    let input:AVAudioPCMBuffer
    let output:AVAudioCompressedBuffer
    var failed=false
    init?(){
        guard let source=AVAudioFormat(commonFormat:.pcmFormatFloat32,sampleRate:48000,channels:2,interleaved:true),
              let target=AVAudioFormat(settings:[
                AVFormatIDKey:kAudioFormatMPEG4AAC,
                AVSampleRateKey:48000,
                AVNumberOfChannelsKey:2
              ]),
              let converter=AVAudioConverter(from:source,to:target),
              let input=AVAudioPCMBuffer(pcmFormat:source,frameCapacity:1024)
        else{return nil}
        converter.bitRate=128000
        converter.primeMethod = .none
        let size=converter.maximumOutputPacketSize
        guard size>0,size<=16384,
              target.streamDescription.pointee.mFormatID==kAudioFormatMPEG4AAC,
              target.streamDescription.pointee.mSampleRate==48000,
              target.streamDescription.pointee.mChannelsPerFrame==2,
              target.streamDescription.pointee.mFramesPerPacket==1024 else{return nil}
        self.converter=converter
        self.input=input
        self.output=AVAudioCompressedBuffer(format:target,packetCapacity:4,maximumPacketSize:size)
    }
    // All calls are serialized by the owner. Pack each raw AAC packet with
    // a four-byte big-endian length for safe cross-language copying.
    func encode(_ pcm:UnsafePointer<Float>,_ frames:Int,
                _ dst:UnsafeMutablePointer<UInt8>,_ capacity:Int)->Int{
        guard !failed,frames==1024,capacity>=4 else{return -1}
        input.frameLength=1024
        let bytes=frames*2*MemoryLayout<Float>.size
        let buffers=UnsafeMutableAudioBufferListPointer(input.mutableAudioBufferList)
        guard buffers.count==1,let memory=buffers[0].mData,
              Int(buffers[0].mDataByteSize)>=bytes else{failed=true;return -1}
        memory.copyMemory(from:UnsafeRawPointer(pcm),byteCount:bytes)
        output.packetCount=0
        output.byteLength=0
        var supplied=false
        var error:NSError?
        let status=converter.convert(to:output,error:&error,withInputFrom:{ _,state in
            if supplied{state.pointee = .noDataNow;return nil}
            supplied=true
            state.pointee = .haveData
            return self.input
        })
        guard status != .error,status != .endOfStream,error==nil,supplied else{failed=true;return -1}
        let count=Int(output.packetCount),length=Int(output.byteLength)
        guard count<=4,length<=65536 else{failed=true;return -1}
        if count==0{return 0}
        guard let descriptions=output.packetDescriptions else{failed=true;return -1}
        let data=output.data.assumingMemoryBound(to:UInt8.self)
        var pos=0
        for i in 0..<count{
            let p=descriptions[i]
            let offset=p.mStartOffset,n=Int(p.mDataByteSize)
            guard offset>=0,offset<=Int64(length),n>0,n<=16384,
                  n<=length-Int(offset),capacity-pos>=4+n,
                  p.mVariableFramesInPacket==0||p.mVariableFramesInPacket==1024 else{failed=true;return -1}
            dst[pos]=UInt8((n>>24)&255);dst[pos+1]=UInt8((n>>16)&255)
            dst[pos+2]=UInt8((n>>8)&255);dst[pos+3]=UInt8(n&255)
            dst.advanced(by:pos+4).update(from:data.advanced(by:Int(offset)),count:n)
            pos+=4+n
        }
        return pos
    }
}
@_cdecl("moobee_aac_create")
public func moobee_aac_create(_ error:UnsafeMutablePointer<CChar>?,_ capacity:Int32)->UnsafeMutableRawPointer?{
    guard let e=AACEncoder() else{aacError(error,capacity,"Native AAC-LC initialization failed");return nil}
    return Unmanaged.passRetained(e).toOpaque()
}
@_cdecl("moobee_aac_encode")
public func moobee_aac_encode(_ handle:UnsafeMutableRawPointer?,_ pcm:UnsafePointer<Float>?,_ frames:Int,
    _ output:UnsafeMutablePointer<UInt8>?,_ capacity:Int,
    _ error:UnsafeMutablePointer<CChar>?,_ errorCapacity:Int32)->Int{
    guard let handle,let pcm,let output else{return -1}
    let n=Unmanaged<AACEncoder>.fromOpaque(handle).takeUnretainedValue().encode(pcm,frames,output,capacity)
    if n<0{aacError(error,errorCapacity,"Native AAC conversion or packet extraction failed")}
    return n
}
@_cdecl("moobee_aac_destroy")
public func moobee_aac_destroy(_ handle:UnsafeMutableRawPointer?){
    if let handle{Unmanaged<AACEncoder>.fromOpaque(handle).release()}
}
