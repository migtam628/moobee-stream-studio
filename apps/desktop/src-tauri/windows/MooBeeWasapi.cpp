#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <mmdeviceapi.h>
#include <audioclient.h>
#include <wrl/client.h>
#include <cstdint>
#include <cstdio>
#include <new>
#include <array>
using Microsoft::WRL::ComPtr;
namespace {
constexpr size_t maxFrames=48000;
struct Source {
    bool com=false,started=false,loopback=false;
    ComPtr<IAudioClient> client;
    ComPtr<IAudioCaptureClient> capture;
    std::array<int16_t,maxFrames*2> queue{};
    size_t head=0,count=0;
    ~Source(){if(started)client->Stop();capture.Reset();client.Reset();if(com)CoUninitialize();}
};
void errorText(char* error,int capacity,const char* stage,HRESULT hr){
    if(error&&capacity>0)snprintf(error,static_cast<size_t>(capacity),"%s (HRESULT 0x%08lX)",stage,static_cast<unsigned long>(hr));
}
HRESULT open(Source& s,bool loopback){
    HRESULT hr=CoInitializeEx(nullptr,COINIT_MULTITHREADED);if(FAILED(hr))return hr;s.com=true;s.loopback=loopback;
    ComPtr<IMMDeviceEnumerator> enumerator;ComPtr<IMMDevice> device;
    hr=CoCreateInstance(__uuidof(MMDeviceEnumerator),nullptr,CLSCTX_ALL,IID_PPV_ARGS(&enumerator));if(FAILED(hr))return hr;
    hr=enumerator->GetDefaultAudioEndpoint(loopback?eRender:eCapture,eConsole,&device);if(FAILED(hr))return hr;
    hr=device->Activate(__uuidof(IAudioClient),CLSCTX_ALL,nullptr,reinterpret_cast<void**>(s.client.GetAddressOf()));if(FAILED(hr))return hr;
    WAVEFORMATEX format{};format.wFormatTag=WAVE_FORMAT_PCM;format.nChannels=2;format.nSamplesPerSec=48000;format.wBitsPerSample=16;format.nBlockAlign=4;format.nAvgBytesPerSec=192000;
    DWORD flags=AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM|AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY|AUDCLNT_STREAMFLAGS_NOPERSIST;
    if(loopback)flags|=AUDCLNT_STREAMFLAGS_LOOPBACK;
    hr=s.client->Initialize(AUDCLNT_SHAREMODE_SHARED,flags,2000000,0,&format,nullptr);if(FAILED(hr))return hr;
    hr=s.client->GetService(IID_PPV_ARGS(&s.capture));if(FAILED(hr))return hr;
    hr=s.client->Start();if(SUCCEEDED(hr))s.started=true;return hr;
}
HRESULT drain(Source& s){
    // No unbounded allocation. Reject queue overrun instead of hiding loss.
    for(int packet=0;packet<128;++packet){
        UINT32 available=0;HRESULT hr=s.capture->GetNextPacketSize(&available);if(FAILED(hr))return hr;
        if(!available)return S_OK;
        BYTE* data=nullptr;UINT32 frames=0;DWORD flags=0;
        hr=s.capture->GetBuffer(&data,&frames,&flags,nullptr,nullptr);if(FAILED(hr))return hr;
        if(frames>maxFrames-s.count||(!(flags&AUDCLNT_BUFFERFLAGS_SILENT)&&!data)){
            s.capture->ReleaseBuffer(frames);return E_FAIL;
        }
        for(UINT32 i=0;i<frames;++i){
            const size_t pos=(s.head+s.count+i)%maxFrames;
            if(flags&AUDCLNT_BUFFERFLAGS_SILENT){s.queue[pos*2]=0;s.queue[pos*2+1]=0;}
            else{CopyMemory(&s.queue[pos*2],data+i*4,4);}
        }
        s.count+=frames;
        hr=s.capture->ReleaseBuffer(frames);if(FAILED(hr))return hr;
    }
    return E_FAIL;
}
}
extern "C" void* moobee_wasapi_open(int loopback,char* error,int capacity){
    auto* s=new(std::nothrow) Source;
    if(!s){errorText(error,capacity,"Allocate WASAPI source",E_OUTOFMEMORY);return nullptr;}
    HRESULT hr=open(*s,loopback!=0);
    if(FAILED(hr)){errorText(error,capacity,"Open default WASAPI endpoint",hr);delete s;return nullptr;}
    return s;
}
extern "C" int moobee_wasapi_read(void* handle,float* out,size_t frames,uint32_t* missing,char* error,int capacity){
    auto& s=*static_cast<Source*>(handle);
    if(frames==0||frames>4800){errorText(error,capacity,"Invalid WASAPI block",E_INVALIDARG);return -1;}
    // One nonblocking poll per media tick; insufficient samples are explicitly counted.
    HRESULT hr=drain(s);if(FAILED(hr)){errorText(error,capacity,"Read WASAPI packet or bounded queue overrun",hr);return -1;}
    size_t take=s.count<frames?s.count:frames;
    for(size_t i=0;i<frames;++i){
        if(i<take){size_t p=(s.head+i)%maxFrames;out[i*2]=s.queue[p*2]/32768.0f;out[i*2+1]=s.queue[p*2+1]/32768.0f;}
        else{out[i*2]=0;out[i*2+1]=0;}
    }
    s.head=(s.head+take)%maxFrames;s.count-=take;*missing=static_cast<uint32_t>(frames-take);
    return 0;
}
extern "C" int moobee_wasapi_discard(void* handle,char* error,int capacity){
    auto& s=*static_cast<Source*>(handle);
    s.head=0;s.count=0;
    HRESULT hr=drain(s);
    s.head=0;s.count=0;
    if(FAILED(hr)){errorText(error,capacity,"Discard paused WASAPI input",hr);return -1;}
    return 0;
}
extern "C" void moobee_wasapi_close(void* handle){delete static_cast<Source*>(handle);}
