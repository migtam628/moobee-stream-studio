// Native Media Foundation sink. Called synchronously on one Rust capture worker.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mfreadwrite.h>
#include <wrl/client.h>
#include <cstdint>
#include <cstdio>
#include <new>
using Microsoft::WRL::ComPtr;
namespace {
struct Writer {
    bool com=false, mf=false, finished=false;
    ComPtr<IMFSinkWriter> sink;
    DWORD stream=0;
    UINT32 width=0,height=0,fps=0,count=0;
    ~Writer() { sink.Reset(); if(mf) MFShutdown(); if(com) CoUninitialize(); }
};
void report(char* error,int capacity,const char* stage,HRESULT hr) {
    if(error && capacity>0) snprintf(error,static_cast<size_t>(capacity),"%s (HRESULT 0x%08lX)",stage,static_cast<unsigned long>(hr));
}
HRESULT configure(Writer& w,const wchar_t* path,UINT32 width,UINT32 height,UINT32 fps,UINT32 bitrate) {
    HRESULT hr=CoInitializeEx(nullptr,COINIT_MULTITHREADED);
    if(FAILED(hr)) return hr;
    w.com=true;
    hr=MFStartup(MF_VERSION); if(FAILED(hr))return hr;
    w.mf=true;
    ComPtr<IMFAttributes> attrs;
    hr=MFCreateAttributes(&attrs,2);if(FAILED(hr))return hr;
    hr=attrs->SetGUID(MF_TRANSCODE_CONTAINERTYPE,MFTranscodeContainerType_MPEG4);if(FAILED(hr))return hr;
    // Do not require an available hardware encoder.
    hr=MFCreateSinkWriterFromURL(path,nullptr,attrs.Get(),&w.sink);if(FAILED(hr))return hr;
    ComPtr<IMFMediaType> output,input;
    hr=MFCreateMediaType(&output);if(FAILED(hr))return hr;
    hr=output->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Video);if(FAILED(hr))return hr;
    hr=output->SetGUID(MF_MT_SUBTYPE,MFVideoFormat_H264);if(FAILED(hr))return hr;
    hr=output->SetUINT32(MF_MT_AVG_BITRATE,bitrate*1000);if(FAILED(hr))return hr;
    hr=output->SetUINT32(MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive);if(FAILED(hr))return hr;
    hr=MFSetAttributeSize(output.Get(),MF_MT_FRAME_SIZE,width,height);if(FAILED(hr))return hr;
    hr=MFSetAttributeRatio(output.Get(),MF_MT_FRAME_RATE,fps,1);if(FAILED(hr))return hr;
    hr=MFSetAttributeRatio(output.Get(),MF_MT_PIXEL_ASPECT_RATIO,1,1);if(FAILED(hr))return hr;
    hr=w.sink->AddStream(output.Get(),&w.stream);if(FAILED(hr))return hr;
    hr=MFCreateMediaType(&input);if(FAILED(hr))return hr;
    hr=input->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Video);if(FAILED(hr))return hr;
    hr=input->SetGUID(MF_MT_SUBTYPE,MFVideoFormat_NV12);if(FAILED(hr))return hr;
    hr=input->SetUINT32(MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive);if(FAILED(hr))return hr;
    hr=MFSetAttributeSize(input.Get(),MF_MT_FRAME_SIZE,width,height);if(FAILED(hr))return hr;
    hr=MFSetAttributeRatio(input.Get(),MF_MT_FRAME_RATE,fps,1);if(FAILED(hr))return hr;
    hr=MFSetAttributeRatio(input.Get(),MF_MT_PIXEL_ASPECT_RATIO,1,1);if(FAILED(hr))return hr;
    hr=input->SetUINT32(MF_MT_DEFAULT_STRIDE,width);if(FAILED(hr))return hr;
    hr=w.sink->SetInputMediaType(w.stream,input.Get(),nullptr);if(FAILED(hr))return hr;
    hr=w.sink->BeginWriting();if(FAILED(hr))return hr;
    w.width=width;w.height=height;w.fps=fps;return S_OK;
}
}
extern "C" void* moobee_win_writer_create(const wchar_t* path,UINT32 width,UINT32 height,UINT32 fps,UINT32 bitrate,char* error,int capacity) {
    auto* w=new(std::nothrow) Writer;
    if(!w){report(error,capacity,"Allocate writer",E_OUTOFMEMORY);return nullptr;}
    HRESULT hr=configure(*w,path,width,height,fps,bitrate);
    if(FAILED(hr)){report(error,capacity,"Initialize Media Foundation H.264 sink",hr);delete w;return nullptr;}
    return w;
}
extern "C" int moobee_win_writer_append(void* handle,const uint8_t* nv12,size_t len,char* error,int capacity) {
    auto& w=*static_cast<Writer*>(handle);
    DWORD bytes=w.width*w.height*3/2;
    if(w.finished||w.count>=w.fps*60||len!=bytes){report(error,capacity,"Invalid frame or frame budget",E_INVALIDARG);return -1;}
    ComPtr<IMFMediaBuffer> buffer;
    HRESULT hr=MFCreateMemoryBuffer(bytes,&buffer);
    if(SUCCEEDED(hr)){
        BYTE* data=nullptr;
        hr=buffer->Lock(&data,nullptr,nullptr);
        if(SUCCEEDED(hr)){CopyMemory(data,nv12,bytes);hr=buffer->Unlock();}
    }
    if(SUCCEEDED(hr))hr=buffer->SetCurrentLength(bytes);
    ComPtr<IMFSample> sample;
    if(SUCCEEDED(hr))hr=MFCreateSample(&sample);
    if(SUCCEEDED(hr))hr=sample->AddBuffer(buffer.Get());
    LONGLONG start=static_cast<LONGLONG>(w.count)*10000000/w.fps;
    LONGLONG end=static_cast<LONGLONG>(w.count+1)*10000000/w.fps;
    if(SUCCEEDED(hr))hr=sample->SetSampleTime(start);
    if(SUCCEEDED(hr))hr=sample->SetSampleDuration(end-start);
    if(SUCCEEDED(hr))hr=w.sink->WriteSample(w.stream,sample.Get());
    if(FAILED(hr)){report(error,capacity,"Write Media Foundation video sample",hr);return -1;}
    ++w.count;return 0;
}
extern "C" int moobee_win_writer_finish(void* handle,char* error,int capacity) {
    auto& w=*static_cast<Writer*>(handle);
    if(w.finished||w.count<2){report(error,capacity,"Invalid finalization state",E_INVALIDARG);return -1;}
    w.finished=true;
    HRESULT hr=w.sink->Finalize();
    if(FAILED(hr)){report(error,capacity,"Finalize Media Foundation MP4",hr);return -1;}
    return 0;
}
extern "C" void moobee_win_writer_destroy(void* handle){delete static_cast<Writer*>(handle);}
extern "C" int moobee_win_disk_space(const wchar_t* path,uint64_t* bytes) {
    ULARGE_INTEGER available{};
    if(!GetDiskFreeSpaceExW(path,&available,nullptr,nullptr))return -1;
    *bytes=available.QuadPart;return 0;
}
