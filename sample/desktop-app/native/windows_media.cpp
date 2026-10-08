#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <Windows.h>
#include "media_jni.h"
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Media.Control.h>
#include <winrt/Windows.Storage.Streams.h>
#include <algorithm>
#include <chrono>
#include <cmath>
#include <cwctype>
#include <memory>

using namespace winrt;
using namespace Windows::Media::Control;
using namespace Windows::Storage::Streams;

namespace {
template<typename T> T await(const Windows::Foundation::IAsyncOperation<T>& operation) {
    if (operation.wait_for(std::chrono::seconds(5)) == Windows::Foundation::AsyncStatus::Started) {
        operation.Cancel();
        throw hresult_error(HRESULT_FROM_WIN32(ERROR_TIMEOUT), L"SMTC operation timed out");
    }
    return operation.GetResults();
}
std::u16string utf16(const hstring& string) {
    return {reinterpret_cast<const char16_t*>(string.data()), string.size()};
}
std::u16string track_id(const GlobalSystemMediaTransportControlsSessionMediaProperties& properties) {
    return utf16(properties.Title()) + u'\0' + utf16(properties.Artist()) + u'\0' + utf16(properties.AlbumTitle());
}
bool apple_music(const hstring& source) {
    std::wstring text(source);
    std::transform(text.begin(), text.end(), text.begin(), [](wchar_t c) { return std::towlower(c); });
    return text.find(L"applemusic") != std::wstring::npos;
}
struct MediaClient {
    GlobalSystemMediaTransportControlsSessionManager manager{nullptr};
    std::u16string artwork_key;
    std::chrono::steady_clock::time_point artwork_read{};
    bool has_artwork = false;
};
GlobalSystemMediaTransportControlsSession matching_session(MediaClient* client,
    const std::u16string& source, const std::u16string& track) {
    const auto session = client->manager.GetCurrentSession();
    if (!session || utf16(session.SourceAppUserModelId()) != source) return nullptr;
    if (track_id(await(session.TryGetMediaPropertiesAsync())) != track) return nullptr;
    return session;
}
void report(JNIEnv* env) {
    try { throw; }
    catch (const hresult_error& failure) { media_error(env, to_string(failure.message()).c_str()); }
    catch (const std::exception& failure) { media_error(env, failure.what()); }
    catch (...) { media_error(env, "Native SMTC call failed"); }
}
}

extern "C" JNIEXPORT jlong JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_create(JNIEnv* env, jobject) {
    bool initialized = false;
    try {
        init_apartment(apartment_type::multi_threaded);
        initialized = true;
        auto client = std::make_unique<MediaClient>();
        client->manager = await(GlobalSystemMediaTransportControlsSessionManager::RequestAsync());
        return reinterpret_cast<jlong>(client.release());
    } catch (...) {
        if (initialized) uninit_apartment();
        report(env);
        return 0;
    }
}

extern "C" JNIEXPORT jobject JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_read(JNIEnv* env, jobject, jlong handle) {
    try {
        auto* client = reinterpret_cast<MediaClient*>(handle);
        const auto session = client->manager.GetCurrentSession();
        if (!session) return nullptr;
        const auto properties = await(session.TryGetMediaPropertiesAsync());
        const auto timeline = session.GetTimelineProperties();
        const auto sampled_at = java_nano_time(env);
        const auto wall_time = clock::now();
        const auto playback = session.GetPlaybackInfo();
        const auto source = session.SourceAppUserModelId();
        const auto track = track_id(properties);
        const bool playing = playback.PlaybackStatus() == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
        double speed = playback.PlaybackRate() ? playback.PlaybackRate().Value() : 1.0;
        if (!std::isfinite(speed) || speed <= 0) speed = 1.0;
        int64_t position = (timeline.Position() - timeline.StartTime()).count() / 10000;
        // Apple Music's manually published timeline uses the dedicated eight-sample
        // clock in Kotlin. Other publishers expose a timestamp suitable for projection.
        if (playing && !apple_music(source)) {
            const auto age = std::chrono::duration_cast<std::chrono::milliseconds>(wall_time - timeline.LastUpdatedTime()).count();
            if (age >= 0 && age < 30000) position += static_cast<int64_t>(age * speed);
        }
        const auto duration = (timeline.EndTime() - timeline.StartTime()).count() / 10000;
        const auto key = utf16(source) + u'\0' + track;
        const auto now = std::chrono::steady_clock::now();
        std::vector<uint8_t> artwork;
        if (key != client->artwork_key || (!client->has_artwork && now - client->artwork_read > std::chrono::seconds(5))) {
            client->artwork_key = key;
            client->artwork_read = now;
            client->has_artwork = false;
            try {
                if (const auto thumbnail = properties.Thumbnail()) {
                    const auto stream = await(thumbnail.OpenReadAsync());
                    if (stream.Size() <= 16 * 1024 * 1024) {
                        const DataReader reader(stream.GetInputStreamAt(0));
                        const auto count = await(reader.LoadAsync(static_cast<uint32_t>(stream.Size())));
                        artwork.resize(count);
                        reader.ReadBytes(artwork);
                        reader.Close();
                    }
                    stream.Close();
                }
                client->has_artwork = !artwork.empty();
            } catch (...) { /* A missing cover must not interrupt the playback clock. */ }
        }
        return media_snapshot(env, utf16(source), track, utf16(properties.Title()), utf16(properties.Artist()),
            std::max<int64_t>(0, position), std::max<int64_t>(0, duration), playing,
            playback.Controls().IsPlaybackPositionEnabled(), static_cast<float>(speed),
            timeline.LastUpdatedTime().time_since_epoch().count(), artwork, {}, sampled_at);
    } catch (...) { report(env); return nullptr; }
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_seek(JNIEnv* env, jobject, jlong handle,
    jstring source, jstring track, jlong position) {
    try {
        const auto session = matching_session(reinterpret_cast<MediaClient*>(handle), native_string(env, source), native_string(env, track));
        if (!session || !session.GetPlaybackInfo().Controls().IsPlaybackPositionEnabled()) return false;
        const auto timeline = session.GetTimelineProperties();
        const auto duration = std::max<int64_t>(0, (timeline.EndTime() - timeline.StartTime()).count() / 10000);
        position = std::max<jlong>(0, position);
        if (duration > 0) position = std::min<jlong>(position, duration);
        return await(session.TryChangePlaybackPositionAsync(timeline.StartTime().count() + position * 10000));
    } catch (...) { report(env); return false; }
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_toggle(JNIEnv* env, jobject, jlong handle,
    jstring source, jstring track) {
    try {
        const auto session = matching_session(reinterpret_cast<MediaClient*>(handle), native_string(env, source), native_string(env, track));
        return session && await(session.TryTogglePlayPauseAsync());
    } catch (...) { report(env); return false; }
}

extern "C" JNIEXPORT void JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_release(JNIEnv*, jobject, jlong handle) {
    delete reinterpret_cast<MediaClient*>(handle);
    uninit_apartment();
}
