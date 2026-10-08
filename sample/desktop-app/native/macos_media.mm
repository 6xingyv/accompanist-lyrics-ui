#include "media_jni.h"
#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <ScriptingBridge/ScriptingBridge.h>
#include <algorithm>
#include <chrono>
#include <memory>

// Native ScriptingBridge selectors from Music/Spotify's scripting dictionaries.
// This sends Apple Events directly; no interpreter or child process is involved.
@protocol AccompanistPlayer
@property(readonly) OSType playerState;
@property(readonly) id currentTrack;
@property double playerPosition;
-(void)playpause;
@end
@protocol AccompanistTrack
@property(readonly) NSString* name;
@property(readonly) NSString* artist;
@property(readonly) NSString* album;
@property(readonly) NSString* persistentID;
@property(readonly) NSString* artworkUrl;
@property(readonly) SBElementArray* artworks;
@end
@protocol AccompanistArtwork
@property(readonly) NSData* rawData;
@end

@interface AccompanistMediaErrors : NSObject<SBApplicationDelegate>
@property(strong) NSError* lastError;
@end
@implementation AccompanistMediaErrors
-(id)eventDidFail:(const AppleEvent*)event withError:(NSError*)error {
    self.lastError = error;
    return nil;
}
@end

namespace {
std::u16string utf16(NSString* string) {
    if (!string) return {};
    std::u16string result(string.length, u'\0');
    [string getCharacters:reinterpret_cast<unichar*>(result.data()) range:NSMakeRange(0, string.length)];
    return result;
}
std::u16string track_id(id<AccompanistTrack> track, bool spotify) {
    NSString* identifier = spotify ? [(SBObject*)track valueForKey:@"id"] : track.persistentID;
    if (identifier.length) return utf16(identifier);
    return utf16(track.name) + u'\0' + utf16(track.artist) + u'\0' + utf16(track.album);
}
struct MediaClient {
    __strong AccompanistMediaErrors* errors = [AccompanistMediaErrors new];
    __strong SBApplication<AccompanistPlayer>* music =
        (SBApplication<AccompanistPlayer>*)[SBApplication applicationWithBundleIdentifier:@"com.apple.Music"];
    __strong SBApplication<AccompanistPlayer>* spotify =
        (SBApplication<AccompanistPlayer>*)[SBApplication applicationWithBundleIdentifier:@"com.spotify.client"];
    std::u16string active_source;
    std::u16string artwork_key;
    std::chrono::steady_clock::time_point artwork_read{};
    bool has_artwork = false;
    MediaClient() {
        music.delegate = errors;
        spotify.delegate = errors;
        // Apple Event timeout is in ticks (60 per second). Keep a stalled player
        // from holding the native worker indefinitely.
        music.timeout = 5 * 60;
        spotify.timeout = 5 * 60;
    }
};
struct Snapshot {
    std::u16string source, track, title, artist, artwork_url;
    int64_t position = 0, duration = 0;
    bool playing = false;
    jlong sampled_at = 0;
};
bool read_player(JNIEnv* env, MediaClient* client, SBApplication<AccompanistPlayer>* app,
    bool spotify, Snapshot& sample, NSString* __strong& failure) {
    if (![app isRunning]) return false; // Never launch a stopped player.
    client->errors.lastError = nil;
    @try {
        const OSType state = app.playerState;
        if (state == 'kPSS') return false;
        id<AccompanistTrack> track = app.currentTrack;
        sample.source = spotify ? u"com.spotify.client" : u"com.apple.Music";
        sample.track = track_id(track, spotify);
        sample.title = utf16(track.name);
        sample.artist = utf16(track.artist);
        // Music returns floating-point seconds; Spotify returns integer milliseconds.
        // KVC boxes each dictionary's native return type, avoiding a shared double ABI.
        NSNumber* duration = [(SBObject*)track valueForKey:@"duration"];
        sample.duration = spotify ? duration.longLongValue : static_cast<int64_t>(duration.doubleValue * 1000.0);
        if (spotify) sample.artwork_url = utf16(track.artworkUrl);
        sample.position = static_cast<int64_t>(app.playerPosition * 1000);
        sample.sampled_at = java_nano_time(env);
        sample.playing = state == 'kPSP';
        if (client->errors.lastError) {
            failure = client->errors.lastError.localizedDescription;
            return false;
        }
        return !sample.title.empty();
    } @catch (NSException* exception) { failure = exception.reason; return false; }
}
SBApplication<AccompanistPlayer>* matching_player(MediaClient* client,
    const std::u16string& source, const std::u16string& track) {
    const bool spotify = source == u"com.spotify.client";
    if (!spotify && source != u"com.apple.Music") return nil;
    auto app = spotify ? client->spotify : client->music;
    if (![app isRunning]) return nil;
    client->errors.lastError = nil;
    if (track_id(app.currentTrack, spotify) != track || client->errors.lastError) return nil;
    return app;
}
void report(JNIEnv* env, NSException* exception) {
    media_error(env, exception.reason.UTF8String ?: "macOS media call failed");
}
}

extern "C" JNIEXPORT jlong JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_create(JNIEnv* env, jobject) {
    return guarded_media<jlong>(env, 0, [&]() -> jlong {
    @autoreleasepool {
        @try { return reinterpret_cast<jlong>(new MediaClient()); }
        @catch (NSException* exception) { report(env, exception); return 0; }
    }
    });
}

extern "C" JNIEXPORT jobject JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_read(JNIEnv* env, jobject, jlong handle) {
    return guarded_media<jobject>(env, nullptr, [&]() -> jobject {
    @autoreleasepool {
        @try {
            auto* client = reinterpret_cast<MediaClient*>(handle);
            Snapshot music, spotify;
            NSString* failure = nil;
            const bool has_music = read_player(env, client, client->music, false, music, failure);
            const bool has_spotify = read_player(env, client, client->spotify, true, spotify, failure);
            Snapshot* selected = nullptr;
            if (has_music && music.playing && client->active_source == music.source) selected = &music;
            else if (has_spotify && spotify.playing && client->active_source == spotify.source) selected = &spotify;
            else if (has_music && music.playing) selected = &music;
            else if (has_spotify && spotify.playing) selected = &spotify;
            else if (has_spotify && client->active_source == spotify.source) selected = &spotify;
            else if (has_music) selected = &music;
            else if (has_spotify) selected = &spotify;
            if (!selected) {
                if (failure) media_error(env, failure.UTF8String);
                return nullptr;
            }
            if (client->active_source != selected->source) {
                client->artwork_key.clear();
                client->has_artwork = false;
            }
            client->active_source = selected->source;
            std::vector<uint8_t> artwork;
            const auto key = selected->source + u'\0' + selected->track;
            const auto now = std::chrono::steady_clock::now();
            if (selected == &music && (key != client->artwork_key ||
                (!client->has_artwork && now - client->artwork_read > std::chrono::seconds(5)))) {
                client->artwork_key = key;
                client->artwork_read = now;
                client->has_artwork = false;
                @try {
                    id<AccompanistTrack> track = client->music.currentTrack;
                    if (track.artworks.count > 0) {
                        id<AccompanistArtwork> first = [track.artworks objectAtIndex:0];
                        NSData* data = first.rawData;
                        if (data.length > 0 && data.length <= 16 * 1024 * 1024) {
                            const auto* bytes = static_cast<const uint8_t*>(data.bytes);
                            artwork.assign(bytes, bytes + data.length);
                            client->has_artwork = true;
                        }
                    }
                } @catch (NSException* ignored) { /* A missing cover must not interrupt the playback clock. */ }
            }
            return media_snapshot(env, selected->source, selected->track, selected->title, selected->artist,
                std::max<int64_t>(0, selected->position), std::max<int64_t>(0, selected->duration),
                selected->playing, true, 1.0f, 0, artwork, selected->artwork_url, selected->sampled_at);
        } @catch (NSException* exception) { report(env, exception); return nullptr; }
    }
    });
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_seek(JNIEnv* env, jobject, jlong handle,
    jstring source, jstring track, jlong position) {
    return guarded_media<jboolean>(env, false, [&]() -> jboolean {
    @autoreleasepool {
        @try {
            auto* client = reinterpret_cast<MediaClient*>(handle);
            auto app = matching_player(client, native_string(env, source), native_string(env, track));
            if (!app) return false;
            app.playerPosition = std::max<jlong>(0, position) / 1000.0;
            if (client->errors.lastError) { media_error(env, client->errors.lastError.localizedDescription.UTF8String); return false; }
            return true;
        } @catch (NSException* exception) { report(env, exception); return false; }
    }
    });
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_toggle(JNIEnv* env, jobject, jlong handle,
    jstring source, jstring track) {
    return guarded_media<jboolean>(env, false, [&]() -> jboolean {
    @autoreleasepool {
        @try {
            auto* client = reinterpret_cast<MediaClient*>(handle);
            auto app = matching_player(client, native_string(env, source), native_string(env, track));
            if (!app) return false;
            [app playpause];
            if (client->errors.lastError) { media_error(env, client->errors.lastError.localizedDescription.UTF8String); return false; }
            return true;
        } @catch (NSException* exception) { report(env, exception); return false; }
    }
    });
}

extern "C" JNIEXPORT void JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_release(JNIEnv*, jobject, jlong handle) {
    @autoreleasepool { delete reinterpret_cast<MediaClient*>(handle); }
}
