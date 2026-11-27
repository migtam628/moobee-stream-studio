// Same-thread native AAC-LC transform; no sink writer, file or network.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mftransform.h>
#include <mferror.h>
#include <wrl/client.h>
#include <cstdint>
#include <cstdio>
#include <new>
using Microsoft::WRL::ComPtr;
namespace {
constexpr DWORD CAP=16384, FRAMES=1024;
struct Encoder{
    bool com=false,mf=false,failed=false;
    ComPtr<IMFTransform> transform;
    uint64_t frames=0;
    ~Encoder(){
        if(transform){transform->ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH,0);transform->ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING,0);}
        transform.Reset();if(mf)MFShutdown();if(com)CoUninitialize();
    }
};
void report(char* error,int capacity,const char* stage,HRESULT hr){
    if(error&&capacity>0)snprintf(error,static_cast<size_t>(capacity),"%s (HRESULT 0x%08lX)",stage,static_cast<unsigned long>(hr));
}
HRESULT audio_type(IMFMediaType** result,const GUID& subtype){
    ComPtr<IMFMediaType> t;HRESULT hr=MFCreateMediaType(&t);if(FAILED(hr))return hr;
    if(FAILED(hr=t->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Audio)))return hr;
    if(FAILED(hr=t->SetGUID(MF_MT_SUBTYPE,subtype)))return hr;
    if(FAILED(hr=t->SetUINT32(MF_MT_AUDIO_NUM_CHANNELS,2)))return hr;
    if(FAILED(hr=t->SetUINT32(MF_MT_AUDIO_SAMPLES_PER_SECOND,48000)))return hr;
    if(FAILED(hr=t->SetUINT32(MF_MT_AUDIO_BITS_PER_SAMPLE,16)))return hr;
    *result=t.Detach();return S_OK;
}
HRESULT configure(Encoder& e){
    HRESULT hr=CoInitializeEx(nullptr,COINIT_MULTITHREADED);if(FAILED(hr))return hr;e.com=true;
    hr=MFStartup(MF_VERSION);if(FAILED(hr))return hr;e.mf=true;
    MFT_REGISTER_TYPE_INFO in{MFMediaType_Audio,MFAudioFormat_PCM},out{MFMediaType_Audio,MFAudioFormat_AAC};
    IMFActivate** list=nullptr;UINT32 count=0;
    hr=MFTEnumEx(MFT_CATEGORY_AUDIO_ENCODER,MFT_ENUM_FLAG_SYNCMFT|MFT_ENUM_FLAG_SORTANDFILTER,&in,&out,&list,&count);
    if(FAILED(hr))return hr;
    HRESULT activated=MF_E_TOPO_CODEC_NOT_FOUND;
    for(UINT32 i=0;i<count;++i){
        if(!e.transform)activated=list[i]->ActivateObject(IID_PPV_ARGS(&e.transform));
        list[i]->Release();
    }
    CoTaskMemFree(list);if(!e.transform)return FAILED(activated)?activated:MF_E_TOPO_CODEC_NOT_FOUND;
    ComPtr<IMFMediaType> input,output;
    if(FAILED(hr=audio_type(&input,MFAudioFormat_PCM)))return hr;
    if(FAILED(hr=input->SetUINT32(MF_MT_AUDIO_BLOCK_ALIGNMENT,4)))return hr;
    if(FAILED(hr=input->SetUINT32(MF_MT_AUDIO_AVG_BYTES_PER_SECOND,192000)))return hr;
    if(FAILED(hr=input->SetUINT32(MF_MT_ALL_SAMPLES_INDEPENDENT,TRUE)))return hr;
    if(FAILED(hr=e.transform->SetInputType(0,input.Get(),0)))return hr;
    if(FAILED(hr=audio_type(&output,MFAudioFormat_AAC)))return hr;
    if(FAILED(hr=output->SetUINT32(MF_MT_AUDIO_AVG_BYTES_PER_SECOND,16000)))return hr;
    if(FAILED(hr=output->SetUINT32(MF_MT_AAC_PAYLOAD_TYPE,0)))return hr;
    if(FAILED(hr=output->SetUINT32(MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION,0x29)))return hr;
    if(FAILED(hr=e.transform->SetOutputType(0,output.Get(),0)))return hr;
    ComPtr<IMFMediaType> actual;if(FAILED(hr=e.transform->GetOutputCurrentType(0,&actual)))return hr;
    BYTE config[64]{};UINT32 n=0;
    if(FAILED(hr=actual->GetBlob(MF_MT_USER_DATA,config,sizeof(config),&n)))return hr;
    // HEAACWAVEINFO trailing fields (12 bytes), then AAC-LC 48 kHz stereo ASC.
    // Fail closed for alternate profiles, channel counts or payload framing.
    if(n!=14||config[0]!=0||config[1]!=0||config[12]!=0x11||config[13]!=0x90)return E_FAIL;
    if(FAILED(hr=e.transform->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,0)))return hr;
    return e.transform->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM,0);
}
HRESULT push(Encoder& e,const int16_t* pcm,size_t frames){
    if(e.failed||!pcm||frames!=FRAMES||e.frames+frames>48000ULL*3600)return E_INVALIDARG;
    ComPtr<IMFMediaBuffer> buffer;HRESULT hr=MFCreateMemoryBuffer(FRAMES*4,&buffer);if(FAILED(hr))return hr;
    BYTE* data=nullptr;if(FAILED(hr=buffer->Lock(&data,nullptr,nullptr)))return hr;
    CopyMemory(data,pcm,FRAMES*4);if(FAILED(hr=buffer->Unlock()))return hr;
    if(FAILED(hr=buffer->SetCurrentLength(FRAMES*4)))return hr;
    ComPtr<IMFSample> sample;if(FAILED(hr=MFCreateSample(&sample)))return hr;
    if(FAILED(hr=sample->AddBuffer(buffer.Get())))return hr;
    LONGLONG start=static_cast<LONGLONG>(e.frames)*10000000/48000;
    LONGLONG end=static_cast<LONGLONG>(e.frames+frames)*10000000/48000;
    if(FAILED(hr=sample->SetSampleTime(start)))return hr;
    if(FAILED(hr=sample->SetSampleDuration(end-start)))return hr;
    if(FAILED(hr=e.transform->ProcessInput(0,sample.Get(),0)))return hr;
    e.frames+=frames;return S_OK;
}
HRESULT poll(Encoder& e,uint8_t* out,DWORD capacity,DWORD* size,LONGLONG* pts){
    if(e.failed||!out||!size||!pts)return E_INVALIDARG;
    *size=0;
    MFT_OUTPUT_STREAM_INFO info{};HRESULT hr=e.transform->GetOutputStreamInfo(0,&info);if(FAILED(hr))return hr;
    if(info.cbSize>CAP||info.cbAlignment>65536)return E_INVALIDARG;
    ComPtr<IMFSample> supplied;ComPtr<IMFMediaBuffer> buffer;MFT_OUTPUT_DATA_BUFFER result{};
    if(!(info.dwFlags&MFT_OUTPUT_STREAM_PROVIDES_SAMPLES)){
        if(FAILED(hr=MFCreateSample(&supplied)))return hr;
        if(info.cbAlignment)hr=MFCreateAlignedMemoryBuffer(CAP,info.cbAlignment-1,&buffer);
        else hr=MFCreateMemoryBuffer(CAP,&buffer);
        if(FAILED(hr))return hr;
        if(FAILED(hr=supplied->AddBuffer(buffer.Get())))return hr;
        result.pSample=supplied.Get();
    }
    DWORD status=0;hr=e.transform->ProcessOutput(0,1,&result,&status);
    if(result.pEvents)result.pEvents->Release();
    ComPtr<IMFSample> encoded;
    if(result.pSample==supplied.Get())encoded=supplied;else if(result.pSample)encoded.Attach(result.pSample);
    if(hr==MF_E_TRANSFORM_NEED_MORE_INPUT)return S_FALSE;
    if(FAILED(hr))return hr;
    if(!encoded)return E_FAIL;
    if(FAILED(hr=encoded->GetSampleTime(pts)))return hr;
    ComPtr<IMFMediaBuffer> contiguous;if(FAILED(hr=encoded->ConvertToContiguousBuffer(&contiguous)))return hr;
    BYTE* data=nullptr;DWORD n=0;if(FAILED(hr=contiguous->Lock(&data,nullptr,&n)))return hr;
    if(!n||n>capacity){contiguous->Unlock();return E_INVALIDARG;}
    CopyMemory(out,data,n);if(FAILED(hr=contiguous->Unlock()))return hr;
    *size=n;return S_OK;
}
}
extern "C" void* moobee_win_aac_create(char* error,int capacity){
    auto* e=new(std::nothrow) Encoder;
    if(!e){report(error,capacity,"Allocate AAC transform",E_OUTOFMEMORY);return nullptr;}
    HRESULT hr=configure(*e);
    if(FAILED(hr)){report(error,capacity,"Initialize raw AAC-LC transform",hr);delete e;return nullptr;}return e;
}
extern "C" int moobee_win_aac_push(void* handle,const int16_t* pcm,size_t frames,char* error,int capacity){
    auto& e=*static_cast<Encoder*>(handle);HRESULT hr=push(e,pcm,frames);
    if(FAILED(hr)){e.failed=true;report(error,capacity,"Submit AAC PCM block",hr);return -1;}return 0;
}
extern "C" int moobee_win_aac_poll(void* handle,uint8_t* out,DWORD capacity,DWORD* size,LONGLONG* pts,char* error,int error_capacity){
    auto& e=*static_cast<Encoder*>(handle);HRESULT hr=poll(e,out,capacity,size,pts);
    if(FAILED(hr)){e.failed=true;report(error,error_capacity,"Read raw AAC packet",hr);return -1;}
    return hr==S_FALSE?0:1;
}
extern "C" void moobee_win_aac_destroy(void* handle){delete static_cast<Encoder*>(handle);}
