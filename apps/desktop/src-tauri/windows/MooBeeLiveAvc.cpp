// Windows direct packet encoder, without a sink writer, file or network.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mftransform.h>
#include <mferror.h>
#include <strmif.h>
#include <codecapi.h>
#include <wrl/client.h>
#include <cstdint>
#include <cstdio>
#include <new>
#include <algorithm>
using Microsoft::WRL::ComPtr;
namespace {
constexpr DWORD CAP=2*1024*1024;
struct Encoder{
    bool com=false,mf=false,failed=false;
    ComPtr<IMFTransform> transform;
    ComPtr<ICodecAPI> codec;
    UINT32 width=0,height=0,fps=0,index=0;
    ~Encoder(){
        if(transform){transform->ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH,0);transform->ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING,0);}
        codec.Reset();transform.Reset();if(mf)MFShutdown();if(com)CoUninitialize();
    }
};
void report(char* error,int capacity,const char* stage,HRESULT hr){
    if(error&&capacity>0)snprintf(error,static_cast<size_t>(capacity),"%s (HRESULT 0x%08lX)",stage,static_cast<unsigned long>(hr));
}
HRESULT set_u32(ICodecAPI* codec,const GUID& key,ULONG v){
    VARIANT value{};value.vt=VT_UI4;value.ulVal=v;return codec->SetValue(&key,&value);
}
HRESULT set_bool(ICodecAPI* codec,const GUID& key,bool v){
    VARIANT value{};value.vt=VT_BOOL;value.boolVal=v?VARIANT_TRUE:VARIANT_FALSE;return codec->SetValue(&key,&value);
}
HRESULT type_for(IMFMediaType** result,const GUID& subtype,UINT32 w,UINT32 h,UINT32 fps){
    ComPtr<IMFMediaType> t;
    HRESULT hr=MFCreateMediaType(&t);if(FAILED(hr))return hr;
    if(FAILED(hr=t->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Video)))return hr;
    if(FAILED(hr=t->SetGUID(MF_MT_SUBTYPE,subtype)))return hr;
    if(FAILED(hr=t->SetUINT32(MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive)))return hr;
    if(FAILED(hr=MFSetAttributeSize(t.Get(),MF_MT_FRAME_SIZE,w,h)))return hr;
    if(FAILED(hr=MFSetAttributeRatio(t.Get(),MF_MT_FRAME_RATE,fps,1)))return hr;
    if(FAILED(hr=MFSetAttributeRatio(t.Get(),MF_MT_PIXEL_ASPECT_RATIO,1,1)))return hr;
    *result=t.Detach();return S_OK;
}
HRESULT configure(Encoder& e,UINT32 w,UINT32 h,UINT32 fps,UINT32 kbps){
    HRESULT hr=CoInitializeEx(nullptr,COINIT_MULTITHREADED);if(FAILED(hr))return hr;e.com=true;
    hr=MFStartup(MF_VERSION);if(FAILED(hr))return hr;e.mf=true;
    MFT_REGISTER_TYPE_INFO in{MFMediaType_Video,MFVideoFormat_NV12},out{MFMediaType_Video,MFVideoFormat_H264};
    IMFActivate** list=nullptr;UINT32 count=0;
    hr=MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER,MFT_ENUM_FLAG_SYNCMFT|MFT_ENUM_FLAG_SORTANDFILTER,&in,&out,&list,&count);
    if(FAILED(hr))return hr;
    HRESULT activated=MF_E_TOPO_CODEC_NOT_FOUND;
    // Select a synchronous system-registered transform; no helper executable.
    for(UINT32 i=0;i<count;++i){
        if(!e.transform)activated=list[i]->ActivateObject(IID_PPV_ARGS(&e.transform));
        list[i]->Release();
    }
    CoTaskMemFree(list);if(!e.transform)return FAILED(activated)?activated:MF_E_TOPO_CODEC_NOT_FOUND;
    if(FAILED(hr=e.transform.As(&e.codec)))return hr;
    if(FAILED(hr=set_bool(e.codec.Get(),CODECAPI_AVLowLatencyMode,true)))return hr;
    if(FAILED(hr=set_u32(e.codec.Get(),CODECAPI_AVEncMPVGOPSize,fps*2)))return hr;
    ComPtr<IMFMediaType> output,input;
    if(FAILED(hr=type_for(&output,MFVideoFormat_H264,w,h,fps)))return hr;
    if(FAILED(hr=output->SetUINT32(MF_MT_AVG_BITRATE,kbps*1000)))return hr;
    // Baseline disallows B-frames. Reject any timestamp reorder below.
    if(FAILED(hr=output->SetUINT32(MF_MT_MPEG2_PROFILE,66)))return hr;
    if(FAILED(hr=e.transform->SetOutputType(0,output.Get(),0)))return hr;
    if(FAILED(hr=type_for(&input,MFVideoFormat_NV12,w,h,fps)))return hr;
    if(FAILED(hr=input->SetUINT32(MF_MT_DEFAULT_STRIDE,w)))return hr;
    if(FAILED(hr=e.transform->SetInputType(0,input.Get(),0)))return hr;
    if(FAILED(hr=e.transform->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,0)))return hr;
    if(FAILED(hr=e.transform->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM,0)))return hr;
    e.width=w;e.height=h;e.fps=fps;return S_OK;
}
HRESULT copy_config(Encoder& e,uint8_t* config,DWORD capacity,DWORD* length){
    *length=0;ComPtr<IMFMediaType> type;
    HRESULT hr=e.transform->GetOutputCurrentType(0,&type);if(FAILED(hr))return hr;
    DWORD size=0;hr=type->GetBlobSize(MF_MT_MPEG_SEQUENCE_HEADER,&size);
    if(hr==MF_E_ATTRIBUTENOTFOUND)return S_OK;
    if(FAILED(hr))return hr;
    if(size>capacity)return E_INVALIDARG;
    return type->GetBlob(MF_MT_MPEG_SEQUENCE_HEADER,config,capacity,length);
}
HRESULT encode(Encoder& e,const uint8_t* nv12,size_t length,bool key,uint8_t* out,DWORD capacity,DWORD* size,uint8_t* config,DWORD config_cap,DWORD* config_len){
    if(e.failed||!nv12||!out||!config||length!=static_cast<size_t>(e.width)*e.height*3/2||e.index>=e.fps*3600)return E_INVALIDARG;
    if(key){HRESULT hr=set_u32(e.codec.Get(),CODECAPI_AVEncVideoForceKeyFrame,1);if(FAILED(hr))return hr;}
    ComPtr<IMFMediaBuffer> input_buffer;HRESULT hr=MFCreateMemoryBuffer(static_cast<DWORD>(length),&input_buffer);if(FAILED(hr))return hr;
    BYTE* data=nullptr;if(FAILED(hr=input_buffer->Lock(&data,nullptr,nullptr)))return hr;
    CopyMemory(data,nv12,length);if(FAILED(hr=input_buffer->Unlock()))return hr;
    if(FAILED(hr=input_buffer->SetCurrentLength(static_cast<DWORD>(length))))return hr;
    ComPtr<IMFSample> sample;if(FAILED(hr=MFCreateSample(&sample)))return hr;
    if(FAILED(hr=sample->AddBuffer(input_buffer.Get())))return hr;
    LONGLONG start=static_cast<LONGLONG>(e.index)*10000000/e.fps;
    LONGLONG end=static_cast<LONGLONG>(e.index+1)*10000000/e.fps;
    if(FAILED(hr=sample->SetSampleTime(start)))return hr;
    if(FAILED(hr=sample->SetSampleDuration(end-start)))return hr;
    if(FAILED(hr=e.transform->ProcessInput(0,sample.Get(),0)))return hr;
    MFT_OUTPUT_STREAM_INFO info{};if(FAILED(hr=e.transform->GetOutputStreamInfo(0,&info)))return hr;
    if(info.cbSize>CAP||info.cbAlignment>65536)return E_INVALIDARG;
    ComPtr<IMFSample> supplied;ComPtr<IMFMediaBuffer> output_buffer;
    MFT_OUTPUT_DATA_BUFFER result{};
    if(!(info.dwFlags&MFT_OUTPUT_STREAM_PROVIDES_SAMPLES)){
        if(FAILED(hr=MFCreateSample(&supplied)))return hr;
        DWORD alloc=std::max<DWORD>(info.cbSize,CAP);
        if(info.cbAlignment){hr=MFCreateAlignedMemoryBuffer(alloc,info.cbAlignment-1,&output_buffer);}
        else{hr=MFCreateMemoryBuffer(alloc,&output_buffer);}
        if(FAILED(hr))return hr;
        if(FAILED(hr=supplied->AddBuffer(output_buffer.Get())))return hr;
        result.pSample=supplied.Get();
    }
    DWORD status=0;
    hr=e.transform->ProcessOutput(0,1,&result,&status);
    if(result.pEvents)result.pEvents->Release();
    ComPtr<IMFSample> encoded;
    if(result.pSample==supplied.Get())encoded=supplied;
    else if(result.pSample)encoded.Attach(result.pSample);
    // Delayed, reordered, partial or dynamic-format output is outside this
    // first direct packet adapter. Fail rather than invent a timestamp.
    if(FAILED(hr))return hr;
    if(!encoded||(result.dwStatus&MFT_OUTPUT_DATA_BUFFER_INCOMPLETE))return E_FAIL;
    LONGLONG pts=0;
    if(FAILED(hr=encoded->GetSampleTime(&pts)))return hr;
    if(pts!=start)return E_FAIL;
    UINT64 dts=0;if(SUCCEEDED(encoded->GetUINT64(MFSampleExtension_DecodeTimestamp,&dts))&&dts!=static_cast<UINT64>(start))return E_FAIL;
    ComPtr<IMFMediaBuffer> contiguous;if(FAILED(hr=encoded->ConvertToContiguousBuffer(&contiguous)))return hr;
    DWORD n=0;if(FAILED(hr=contiguous->Lock(&data,nullptr,&n)))return hr;
    if(!n||n>capacity){contiguous->Unlock();return E_INVALIDARG;}
    CopyMemory(out,data,n);if(FAILED(hr=contiguous->Unlock()))return hr;
    if(FAILED(hr=copy_config(e,config,config_cap,config_len)))return hr;
    *size=n;++e.index;return S_OK;
}
}
extern "C" void* moobee_win_avc_create(UINT32 w,UINT32 h,UINT32 fps,UINT32 kbps,char* error,int capacity){
    auto* e=new(std::nothrow) Encoder;
    if(!e){report(error,capacity,"Allocate AVC transform",E_OUTOFMEMORY);return nullptr;}
    HRESULT hr=configure(*e,w,h,fps,kbps);
    if(FAILED(hr)){report(error,capacity,"Initialize synchronous AVC transform",hr);delete e;return nullptr;}
    return e;
}
extern "C" int moobee_win_avc_encode(void* handle,const uint8_t* nv12,size_t length,int force_key,uint8_t* out,DWORD capacity,DWORD* size,uint8_t* config,DWORD config_cap,DWORD* config_len,char* error,int error_capacity){
    auto& e=*static_cast<Encoder*>(handle);
    HRESULT hr=encode(e,nv12,length,force_key!=0,out,capacity,size,config,config_cap,config_len);
    if(FAILED(hr)){e.failed=true;report(error,error_capacity,"Encode synchronous AVC access unit",hr);return -1;}return 0;
}
extern "C" void moobee_win_avc_destroy(void* handle){delete static_cast<Encoder*>(handle);}
