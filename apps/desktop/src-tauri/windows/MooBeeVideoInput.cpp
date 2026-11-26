#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mfreadwrite.h>
#include <mferror.h>
#include <wrl/client.h>
#include <atomic>
#include <mutex>
#include <cstdint>
#include <cstdio>
#include <new>
#include <cwchar>
#include <cstdlib>
using Microsoft::WRL::ComPtr;
namespace {
constexpr UINT32 maxW=1280,maxH=720;
constexpr DWORD stream=MF_SOURCE_READER_FIRST_VIDEO_STREAM;
void errorText(char* error,int capacity,const char* stage,HRESULT hr){
    if(error&&capacity>0)snprintf(error,static_cast<size_t>(capacity),"%s (HRESULT 0x%08lX)",stage,static_cast<unsigned long>(hr));
}
struct Runtime {
    bool com=false,mf=false;
    HRESULT start(){
        HRESULT hr=CoInitializeEx(nullptr,COINIT_MULTITHREADED);if(FAILED(hr))return hr;com=true;
        hr=MFStartup(MF_VERSION,MFSTARTUP_FULL);if(SUCCEEDED(hr))mf=true;return hr;
    }
    ~Runtime(){if(mf)MFShutdown();if(com)CoUninitialize();}
};
struct Activations {
    IMFActivate** items=nullptr;UINT32 count=0;
    ~Activations(){if(items){for(UINT32 i=0;i<count;++i)if(items[i])items[i]->Release();CoTaskMemFree(items);}}
};
struct TaskString { LPWSTR value=nullptr; ~TaskString(){CoTaskMemFree(value);} };
using DeviceCallback=int(*)(void*,const wchar_t*,size_t,const wchar_t*,size_t,int);
HRESULT enumerate(DeviceCallback emit,void* context){
    Runtime runtime;HRESULT hr=runtime.start();if(FAILED(hr))return hr;
    ComPtr<IMFAttributes> attrs;hr=MFCreateAttributes(&attrs,1);if(FAILED(hr))return hr;
    hr=attrs->SetGUID(MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID);if(FAILED(hr))return hr;
    Activations devices;hr=MFEnumDeviceSources(attrs.Get(),&devices.items,&devices.count);if(FAILED(hr))return hr;
    if(devices.count>256)return E_FAIL;
    for(UINT32 i=0;i<devices.count;++i){
        TaskString id,name;UINT32 idLen=0,nameLen=0;
        hr=devices.items[i]->GetAllocatedString(MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,&id.value,&idLen);if(FAILED(hr))return hr;
        hr=devices.items[i]->GetAllocatedString(MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,&name.value,&nameLen);if(FAILED(hr))return hr;
        if(!id.value||!name.value||emit(context,id.value,idLen,name.value,nameLen,0)!=0)return E_FAIL;
    }
    return S_OK;
}
// One outstanding asynchronous read. No capture device can enqueue unlimited frames.
class Callback final : public IMFSourceReaderCallback {
    std::atomic<ULONG> refs{1};
public:
    std::mutex mutex;
    bool pending=false,closed=false;
    HRESULT status=S_OK;
    ComPtr<IMFSample> latest;
    STDMETHODIMP QueryInterface(REFIID iid,void** out) override {
        if(!out)return E_POINTER;
        *out=nullptr;
        if(iid==__uuidof(IUnknown)||iid==__uuidof(IMFSourceReaderCallback)){*out=static_cast<IMFSourceReaderCallback*>(this);AddRef();return S_OK;}
        return E_NOINTERFACE;
    }
    STDMETHODIMP_(ULONG) AddRef() override {return ++refs;}
    STDMETHODIMP_(ULONG) Release() override {ULONG count=--refs;if(!count)delete this;return count;}
    STDMETHODIMP OnReadSample(HRESULT hr,DWORD,DWORD flags,LONGLONG,IMFSample* sample) override {
        std::lock_guard<std::mutex> guard(mutex);pending=false;
        if(closed)return S_OK;
        if(FAILED(hr)){status=hr;return S_OK;}
        if(flags&(MF_SOURCE_READERF_ERROR|MF_SOURCE_READERF_ENDOFSTREAM|MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED|MF_SOURCE_READERF_NATIVEMEDIATYPECHANGED)){
            status=E_FAIL;return S_OK;
        }
        latest=sample;return S_OK;
    }
    STDMETHODIMP OnEvent(DWORD,IMFMediaEvent*) override{return S_OK;}
    STDMETHODIMP OnFlush(DWORD) override {std::lock_guard<std::mutex> guard(mutex);pending=false;latest.Reset();return S_OK;}
};
struct Camera {
    Runtime runtime;
    ComPtr<IMFMediaSource> source;
    ComPtr<IMFSourceReader> reader;
    ComPtr<Callback> callback;
    UINT32 width=0,height=0;LONG stride=0;
    ~Camera(){
        if(callback){std::lock_guard<std::mutex> guard(callback->mutex);callback->closed=true;callback->latest.Reset();}
        if(reader)reader->Flush(stream);
        reader.Reset();if(source)source->Shutdown();source.Reset();callback.Reset();
    }
};
HRESULT open(Camera& camera,const wchar_t* id){
    if(!id||!*id)return E_INVALIDARG;
    HRESULT hr=camera.runtime.start();if(FAILED(hr))return hr;
    ComPtr<IMFAttributes> device;hr=MFCreateAttributes(&device,2);if(FAILED(hr))return hr;
    hr=device->SetGUID(MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID);if(FAILED(hr))return hr;
    hr=device->SetString(MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,id);if(FAILED(hr))return hr;
    hr=MFCreateDeviceSource(device.Get(),&camera.source);if(FAILED(hr))return hr;
    auto* cb=new(std::nothrow) Callback;if(!cb)return E_OUTOFMEMORY;camera.callback.Attach(cb);
    ComPtr<IMFAttributes> attrs;hr=MFCreateAttributes(&attrs,3);if(FAILED(hr))return hr;
    hr=attrs->SetUnknown(MF_SOURCE_READER_ASYNC_CALLBACK,cb);if(FAILED(hr))return hr;
    hr=attrs->SetUINT32(MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,TRUE);if(FAILED(hr))return hr;
    hr=MFCreateSourceReaderFromMediaSource(camera.source.Get(),attrs.Get(),&camera.reader);if(FAILED(hr))return hr;
    hr=camera.reader->SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS,FALSE);if(FAILED(hr))return hr;
    hr=camera.reader->SetStreamSelection(stream,TRUE);if(FAILED(hr))return hr;
    // Prefer the largest bounded native mode, then try other bounded modes.
    // Never request unbounded 4K raw frames; unsupported devices fail explicitly.
    UINT32 best=UINT32_MAX,bestPixels=0;
    for(DWORD i=0;i<256;++i){
        ComPtr<IMFMediaType> type;hr=camera.reader->GetNativeMediaType(stream,i,&type);
        if(hr==MF_E_NO_MORE_TYPES)break;if(FAILED(hr))return hr;
        UINT32 w=0,h=0;if(FAILED(MFGetAttributeSize(type.Get(),MF_MT_FRAME_SIZE,&w,&h)))continue;
        if(w&&h&&w<=maxW&&h<=maxH&&w*h>bestPixels){best=i;bestPixels=w*h;}
    }
    if(best==UINT32_MAX)return MF_E_INVALIDMEDIATYPE;
    ComPtr<IMFMediaType> native;hr=camera.reader->GetNativeMediaType(stream,best,&native);if(FAILED(hr))return hr;
    hr=camera.reader->SetCurrentMediaType(stream,nullptr,native.Get());if(FAILED(hr))return hr;
    hr=MFGetAttributeSize(native.Get(),MF_MT_FRAME_SIZE,&camera.width,&camera.height);if(FAILED(hr))return hr;
    ComPtr<IMFMediaType> output;hr=MFCreateMediaType(&output);if(FAILED(hr))return hr;
    hr=output->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Video);if(FAILED(hr))return hr;
    hr=output->SetGUID(MF_MT_SUBTYPE,MFVideoFormat_RGB32);if(FAILED(hr))return hr;
    hr=MFSetAttributeSize(output.Get(),MF_MT_FRAME_SIZE,camera.width,camera.height);if(FAILED(hr))return hr;
    hr=camera.reader->SetCurrentMediaType(stream,nullptr,output.Get());if(FAILED(hr))return hr;
    ComPtr<IMFMediaType> actual;hr=camera.reader->GetCurrentMediaType(stream,&actual);if(FAILED(hr))return hr;
    GUID subtype{};hr=actual->GetGUID(MF_MT_SUBTYPE,&subtype);if(FAILED(hr)||subtype!=MFVideoFormat_RGB32)return MF_E_INVALIDMEDIATYPE;
    hr=MFGetAttributeSize(actual.Get(),MF_MT_FRAME_SIZE,&camera.width,&camera.height);if(FAILED(hr))return hr;
    if(!camera.width||!camera.height||camera.width>maxW||camera.height>maxH)return MF_E_INVALIDMEDIATYPE;
    UINT32 attr=0;if(SUCCEEDED(actual->GetUINT32(MF_MT_DEFAULT_STRIDE,&attr)))camera.stride=static_cast<LONG>(attr);
    else {hr=MFGetStrideForBitmapInfoHeader(MFVideoFormat_RGB32.Data1,camera.width,&camera.stride);if(FAILED(hr))return hr;}
    return S_OK;
}
bool validStride(LONG stride,UINT32 width){
    const int64_t pitch=stride<0?-static_cast<int64_t>(stride):stride;
    return pitch>=static_cast<int64_t>(width)*4&&pitch<=16384;
}
void bgraToRgba(const BYTE* row,LONG stride,UINT32 w,UINT32 h,uint8_t* out){
    for(UINT32 y=0;y<h;++y){
        const BYTE* src=row+static_cast<ptrdiff_t>(y)*stride;
        for(UINT32 x=0;x<w;++x){const BYTE* pixel=src+x*4;*out++=pixel[2];*out++=pixel[1];*out++=pixel[0];*out++=255;}
    }
}
HRESULT copySample(Camera& c,IMFSample* sample,uint8_t* out,size_t capacity){
    if(capacity<static_cast<size_t>(c.width)*c.height*4)return E_INVALIDARG;
    ComPtr<IMFMediaBuffer> buffer;HRESULT hr=sample->ConvertToContiguousBuffer(&buffer);if(FAILED(hr))return hr;
    ComPtr<IMF2DBuffer> plane;
    if(SUCCEEDED(buffer.As(&plane))){
        BYTE* scan=nullptr;LONG pitch=0;hr=plane->Lock2D(&scan,&pitch);if(FAILED(hr))return hr;
        HRESULT result=E_FAIL;
        if(scan&&validStride(pitch,c.width)){bgraToRgba(scan,pitch,c.width,c.height,out);result=S_OK;}
        plane->Unlock2D();return result;
    }
    BYTE* data=nullptr;DWORD length=0;hr=buffer->Lock(&data,nullptr,&length);if(FAILED(hr))return hr;
    const int64_t pitch=c.stride<0?-static_cast<int64_t>(c.stride):c.stride;
    HRESULT result=E_FAIL;
    if(data&&validStride(c.stride,c.width)&&static_cast<uint64_t>(pitch)*(c.height-1)+c.width*4<=length){
        const BYTE* top=c.stride<0?data+pitch*(c.height-1):data;
        bgraToRgba(top,c.stride,c.width,c.height,out);result=S_OK;
    }
    buffer->Unlock();return result;
}
}
extern "C" int moobee_win_video_list(DeviceCallback emit,void* context,char* error,int capacity){
    if(!emit||!context)return -1;
    HRESULT hr=enumerate(emit,context);
    if(FAILED(hr)){errorText(error,capacity,"Enumerate Media Foundation video sources",hr);return -1;}return 0;
}
extern "C" void* moobee_win_video_open(const wchar_t* id,char* error,int capacity){
    auto* camera=new(std::nothrow) Camera;if(!camera)return nullptr;
    HRESULT hr=open(*camera,id);if(FAILED(hr)){errorText(error,capacity,"Open selected video input (720p or lower RGB32 mode)",hr);delete camera;return nullptr;}
    return camera;
}
extern "C" int moobee_win_video_poll(void* handle,uint8_t* out,size_t capacity,int* width,int* height,char* error,int errorCapacity){
    auto& c=*static_cast<Camera*>(handle);ComPtr<IMFSample> sample;bool request=false;HRESULT status;
    {
        std::lock_guard<std::mutex> guard(c.callback->mutex);
        status=c.callback->status;
        if(SUCCEEDED(status)){sample.Swap(c.callback->latest);if(!c.callback->pending){c.callback->pending=true;request=true;}}
    }
    if(FAILED(status)){errorText(error,errorCapacity,"Video source stopped or changed format",status);return -1;}
    if(request){
        status=c.reader->ReadSample(stream,0,nullptr,nullptr,nullptr,nullptr);
        if(FAILED(status)){errorText(error,errorCapacity,"Request asynchronous video frame",status);return -1;}
    }
    if(!sample)return 0;
    status=copySample(c,sample.Get(),out,capacity);
    if(FAILED(status)){errorText(error,errorCapacity,"Copy bounded video frame",status);return -1;}
    *width=static_cast<int>(c.width);*height=static_cast<int>(c.height);return 1;
}
extern "C" void moobee_win_video_close(void* handle){delete static_cast<Camera*>(handle);}
