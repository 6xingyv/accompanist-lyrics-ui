#include "media_jni.h"
#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <ScriptingBridge/ScriptingBridge.h>
#include <algorithm>
#include <chrono>
#include <dlfcn.h>
#include <memory>
#include <mutex>

// ---------------------------------------------------------------------------
// MediaRemote (private framework) — system-wide Now Playing.
// Covers Music, Spotify, browsers and any app that publishes to the system
// transport. Loaded dynamically; absence only disables that source.
// ---------------------------------------------------------------------------
namespace mr {
using GetInfoFn = void (*)(dispatch_queue_t, void (^)(CFDictionaryRef));
using GetPlayingFn = void (*)(dispatch_queue_t, void (^)(Boolean));
using GetPidFn = void (*)(dispatch_queue_t, void (^)(int));
using RegisterFn = void (*)(dispatch_queue_t);
using UnregisterFn = void (*)(void);
using SendCommandFn = Boolean (*)(int, id);
using SetElapsedFn = void (*)(double);

GetInfoFn get_info = nullptr;
GetPlayingFn get_playing = nullptr;
GetPidFn get_pid = nullptr;
RegisterFn register_notifications = nullptr;
UnregisterFn unregister_notifications = nullptr;
SendCommandFn send_command = nullptr;
SetElapsedFn set_elapsed = nullptr;
bool loaded = false;

enum Command {
    kPlay = 0,
    kPause = 1,
    kTogglePlayPause = 2,
    kStop = 3,
    kNextTrack = 4,
    kPreviousTrack = 5,
};

void load() {
    static std::once_flag once;
    std::call_once(once, [] {
        void* handle = dlopen(
            "/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote",
            RTLD_LAZY);
        if (!handle) return;
        get_info = reinterpret_cast<GetInfoFn>(
            dlsym(handle, "MRMediaRemoteGetNowPlayingInfo"));
        get_playing = reinterpret_cast<GetPlayingFn>(
            dlsym(handle, "MRMediaRemoteGetNowPlayingApplicationIsPlaying"));
        get_pid = reinterpret_cast<GetPidFn>(
            dlsym(handle, "MRMediaRemoteGetNowPlayingApplicationPID"));
        register_notifications = reinterpret_cast<RegisterFn>(
            dlsym(handle, "MRMediaRemoteRegisterForNowPlayingNotifications"));
        unregister_notifications = reinterpret_cast<UnregisterFn>(
            dlsym(handle, "MRMediaRemoteUnregisterForNowPlayingNotifications"));
        send_command = reinterpret_cast<SendCommandFn>(
            dlsym(handle, "MRMediaRemoteSendCommand"));
        set_elapsed = reinterpret_cast<SetElapsedFn>(
            dlsym(handle, "MRMediaRemoteSetElapsedTime"));
        loaded = get_info && get_playing;
        // Keep the handle; some OS releases return a shared image.
    });
}

dispatch_queue_t queue() {
    static dispatch_queue_t q = dispatch_queue_create(
        "com.mocharealm.accompanist.media.mediaremote", DISPATCH_QUEUE_SERIAL);
    return q;
}
}  // namespace mr

// Native ScriptingBridge selectors from Music/Spotify's scripting dictionaries.
// This sends Apple Events directly; no interpreter or child process is involved.
@protocol AccompanistPlayer
@property(readonly) id playerState;
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

// Music/Spotify scripting-dictionary enum values (four-char codes).
constexpr NSInteger kStateStopped = 'kPSS';
constexpr NSInteger kStatePlaying = 'kPSP';
constexpr NSInteger kStatePaused = 'kPSp';

std::u16string utf16(NSString* string) {
    if (!string) return {};
    std::u16string result(string.length, u'\0');
    [string getCharacters:reinterpret_cast<unichar*>(result.data())
                    range:NSMakeRange(0, string.length)];
    return result;
}

NSInteger player_state_code(id<AccompanistPlayer> app) {
    // KVC boxes the dictionary's native enum; a raw OSType property is not
    // reliable on arm64. Accept both numeric four-char codes and name strings.
    id value = nil;
    @try { value = [(SBObject*)app valueForKey:@"playerState"]; }
    @catch (NSException*) { return kStateStopped; }
    if ([value isKindOfClass:[NSNumber class]]) {
        return static_cast<NSInteger>([value longLongValue]);
    }
    if ([value isKindOfClass:[NSString class]]) {
        NSString* name = static_cast<NSString*>(value);
        if ([name isEqualToString:@"playing"] || [name isEqualToString:@"kPSP"]) return kStatePlaying;
        if ([name isEqualToString:@"paused"] || [name isEqualToString:@"kPSp"]) return kStatePaused;
        return kStateStopped;
    }
    if (value != nil) {
        // Unexpected boxing; interpret the first four bytes as a code.
        return static_cast<NSInteger>([value longLongValue]);
    }
    return kStateStopped;
}

std::u16string track_id(id<AccompanistTrack> track, bool spotify) {
    if (!track) return {};
    NSString* identifier = nil;
    @try {
        identifier = spotify ? [(SBObject*)track valueForKey:@"id"] : track.persistentID;
    } @catch (NSException*) { identifier = nil; }
    if (identifier.length) return utf16(identifier);
    return utf16(track.name) + u'\0' + utf16(track.artist) + u'\0' + utf16(track.album);
}

struct Snapshot {
    std::u16string source, track, title, artist, album, artwork_url;
    int64_t position = 0, duration = 0;
    bool playing = false;
    bool can_seek = true;
    float speed = 1.0f;
    jlong sampled_at = 0;
    std::vector<uint8_t> artwork;
};

SBApplication<AccompanistPlayer>* open_player(NSArray<NSString*>* bundle_ids,
                                              AccompanistMediaErrors* errors) {
    for (NSString* bundle_id in bundle_ids) {
        SBApplication* raw = [SBApplication applicationWithBundleIdentifier:bundle_id];
        if (!raw) continue;
        SBApplication<AccompanistPlayer>* app =
            static_cast<SBApplication<AccompanistPlayer>*>(raw);
        app.delegate = errors;
        // Apple Event timeout is in ticks (60 per second). Keep a stalled player
        // from holding the native worker indefinitely.
        app.timeout = 5 * 60;
        return app;
    }
    return nil;
}

struct MediaClient {
    __strong AccompanistMediaErrors* errors = [AccompanistMediaErrors new];
    __strong SBApplication<AccompanistPlayer>* music;
    __strong SBApplication<AccompanistPlayer>* spotify;
    std::u16string active_source;
    std::u16string last_system_track;
    std::vector<uint8_t> last_system_artwork;
    std::u16string artwork_key;
    std::chrono::steady_clock::time_point artwork_read{};
    bool has_artwork = false;
    MediaClient() {
        music = open_player(@[@"com.apple.Music", @"com.apple.iTunes"], errors);
        spotify = open_player(@[@"com.spotify.client"], errors);
        mr::load();
        if (mr::loaded && mr::register_notifications) {
            mr::register_notifications(mr::queue());
        }
    }
    ~MediaClient() {
        if (mr::loaded && mr::unregister_notifications) {
            mr::unregister_notifications();
        }
    }
};

bool read_scripted_player(JNIEnv* env, MediaClient* client,
                          SBApplication<AccompanistPlayer>* app, bool spotify,
                          const std::u16string& source_id, Snapshot& sample,
                          NSString* __strong& failure) {
    if (!app || ![app isRunning]) return false;  // Never launch a stopped player.
    client->errors.lastError = nil;
    @try {
        const NSInteger state = player_state_code(app);
        if (state == kStateStopped) return false;
        id<AccompanistTrack> track = app.currentTrack;
        if (!track) return false;
        // Reject radio/streaming empty entries that have no title.
        sample.source = source_id;
        sample.track = track_id(track, spotify);
        sample.title = utf16(track.name);
        sample.artist = utf16(track.artist);
        sample.album = utf16(track.album);
        // Music returns floating-point seconds; Spotify returns integer
        // milliseconds. KVC boxes each dictionary's native return type.
        NSNumber* duration = [(SBObject*)track valueForKey:@"duration"];
        if ([duration isKindOfClass:[NSNumber class]]) {
            sample.duration = spotify
                ? duration.longLongValue
                : static_cast<int64_t>(duration.doubleValue * 1000.0);
        }
        if (spotify) sample.artwork_url = utf16(track.artworkUrl);
        sample.position = static_cast<int64_t>(app.playerPosition * 1000);
        sample.sampled_at = java_nano_time(env);
        sample.playing = state == kStatePlaying;
        sample.can_seek = true;
        if (client->errors.lastError) {
            failure = client->errors.lastError.localizedDescription;
            return false;
        }
        return !sample.title.empty();
    } @catch (NSException* exception) {
        failure = exception.reason;
        return false;
    }
}

// System-wide Now Playing via MediaRemote. Synchronous wait on the caller's
// native worker; callbacks are delivered on mr::queue().
bool read_system_now_playing(JNIEnv* env, Snapshot& sample) {
    if (!mr::loaded || !mr::get_info || !mr::get_playing) return false;

    __block NSDictionary* info = nil;
    __block Boolean playing = false;
    __block int pid = -1;

    dispatch_group_t group = dispatch_group_create();
    dispatch_queue_t q = mr::queue();

    dispatch_group_enter(group);
    mr::get_info(q, ^(CFDictionaryRef dict) {
        if (dict) info = [(__bridge NSDictionary*)dict copy];
        dispatch_group_leave(group);
    });
    dispatch_group_enter(group);
    mr::get_playing(q, ^(Boolean value) {
        playing = value;
        dispatch_group_leave(group);
    });
    if (mr::get_pid) {
        dispatch_group_enter(group);
        mr::get_pid(q, ^(int value) {
            pid = value;
            dispatch_group_leave(group);
        });
    }

    const long timed_out = dispatch_group_wait(
        group, dispatch_time(DISPATCH_TIME_NOW, int64_t(2 * NSEC_PER_SEC)));
    if (timed_out != 0 || !info) return false;

    NSString* title = info[@"kMRMediaRemoteNowPlayingInfoTitle"];
    if (![title isKindOfClass:[NSString class]] || title.length == 0) return false;

    NSString* artist = info[@"kMRMediaRemoteNowPlayingInfoArtist"];
    NSString* album = info[@"kMRMediaRemoteNowPlayingInfoAlbum"];
    NSNumber* duration = info[@"kMRMediaRemoteNowPlayingInfoDuration"];
    NSNumber* elapsed = info[@"kMRMediaRemoteNowPlayingInfoElapsedTime"];
    NSDate* timestamp = info[@"kMRMediaRemoteNowPlayingInfoTimestamp"];
    id unique = info[@"kMRMediaRemoteNowPlayingInfoUniqueIdentifier"];
    NSData* artwork = info[@"kMRMediaRemoteNowPlayingInfoArtworkData"];
    NSNumber* rate = info[@"kMRMediaRemoteNowPlayingInfoPlaybackRate"];

    sample.title = utf16(title);
    sample.artist = [artist isKindOfClass:[NSString class]] ? utf16(artist) : std::u16string{};
    sample.album = [album isKindOfClass:[NSString class]] ? utf16(album) : std::u16string{};

    if ([unique isKindOfClass:[NSString class]]) {
        sample.track = utf16(static_cast<NSString*>(unique));
    } else if ([unique isKindOfClass:[NSNumber class]]) {
        sample.track = utf16([[unique description] copy]);
    } else {
        sample.track = sample.title + u'\0' + sample.artist + u'\0' + sample.album;
    }

    if ([duration isKindOfClass:[NSNumber class]]) {
        sample.duration = static_cast<int64_t>(duration.doubleValue * 1000.0);
    }
    // Prefer timestamp + elapsed so the clock does not stall between polls.
    if ([elapsed isKindOfClass:[NSNumber class]]) {
        double position_seconds = elapsed.doubleValue;
        if ([timestamp isKindOfClass:[NSDate class]]) {
            const double skew = -[timestamp timeIntervalSinceNow];
            if (skew > -1.0 && skew < 30.0) position_seconds += skew;
        }
        sample.position = static_cast<int64_t>(position_seconds * 1000.0);
    }
    if ([rate isKindOfClass:[NSNumber class]]) {
        sample.speed = rate.floatValue;
    }
    sample.playing = playing != 0;
    sample.can_seek = mr::set_elapsed != nullptr;
    sample.sampled_at = java_nano_time(env);

    if ([artwork isKindOfClass:[NSData class]] && artwork.length > 0 &&
        artwork.length <= 16 * 1024 * 1024) {
        const auto* bytes = static_cast<const uint8_t*>(artwork.bytes);
        sample.artwork.assign(bytes, bytes + artwork.length);
    }

    NSString* source = @"system.nowplaying";
    if (pid > 0) {
        NSRunningApplication* running =
            [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
        if (running.bundleIdentifier.length > 0) {
            source = running.bundleIdentifier;
        }
    }
    sample.source = utf16(source);
    return true;
}

void read_music_artwork(MediaClient* client, const Snapshot& selected,
                        std::vector<uint8_t>& artwork) {
    const auto key = selected.source + u'\0' + selected.track;
    const auto now = std::chrono::steady_clock::now();
    if (key == client->artwork_key &&
        (client->has_artwork || now - client->artwork_read <= std::chrono::seconds(5))) {
        return;
    }
    client->artwork_key = key;
    client->artwork_read = now;
    client->has_artwork = false;
    if (!client->music || ![client->music isRunning]) return;
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
    } @catch (NSException*) {
        /* A missing cover must not interrupt the playback clock. */
    }
}

SBApplication<AccompanistPlayer>* matching_scripted_player(
    MediaClient* client, const std::u16string& source, const std::u16string& track) {
    SBApplication<AccompanistPlayer>* app = nullptr;
    bool spotify = false;
    if (source == u"com.spotify.client") {
        app = client->spotify;
        spotify = true;
    } else if (source == u"com.apple.Music" || source == u"com.apple.iTunes") {
        app = client->music;
    } else {
        return nil;
    }
    if (!app || ![app isRunning]) return nil;
    client->errors.lastError = nil;
    if (track_id(app.currentTrack, spotify) != track || client->errors.lastError) {
        return nil;
    }
    return app;
}

bool system_track_matches(const MediaClient* client, const std::u16string& track) {
    return !client->last_system_track.empty() && client->last_system_track == track;
}

void report(JNIEnv* env, NSException* exception) {
    media_error(env, exception.reason.UTF8String ?: "macOS media call failed");
}

}  // namespace

extern "C" JNIEXPORT jlong JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_create(
    JNIEnv* env, jobject) {
    return guarded_media<jlong>(env, 0, [&]() -> jlong {
        @autoreleasepool {
            @try {
                return reinterpret_cast<jlong>(new MediaClient());
            } @catch (NSException* exception) {
                report(env, exception);
                return 0;
            }
        }
    });
}

extern "C" JNIEXPORT jobject JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_read(
    JNIEnv* env, jobject, jlong handle) {
    return guarded_media<jobject>(env, nullptr, [&]() -> jobject {
        @autoreleasepool {
            @try {
                auto* client = reinterpret_cast<MediaClient*>(handle);
                Snapshot system, music, spotify;
                NSString* sb_failure = nil;
                const bool has_system = read_system_now_playing(env, system);
                const bool has_music = read_scripted_player(
                    env, client, client->music, false, u"com.apple.Music", music,
                    sb_failure);
                const bool has_spotify = read_scripted_player(
                    env, client, client->spotify, true, u"com.spotify.client", spotify,
                    sb_failure);

                // Prefer the system Now Playing source when it is playing; it
                // covers every app. Fall back to ScriptingBridge for richer IDs
                // and for paused/stopped-poll details on Music/Spotify.
                Snapshot* selected = nullptr;
                if (has_system && system.playing) {
                    selected = &system;
                } else if (has_music && music.playing &&
                           client->active_source == music.source) {
                    selected = &music;
                } else if (has_spotify && spotify.playing &&
                           client->active_source == spotify.source) {
                    selected = &spotify;
                } else if (has_music && music.playing) {
                    selected = &music;
                } else if (has_spotify && spotify.playing) {
                    selected = &spotify;
                } else if (has_system && system.playing) {
                    selected = &system;
                } else if (has_spotify && client->active_source == spotify.source) {
                    selected = &spotify;
                } else if (has_music && client->active_source == music.source) {
                    selected = &music;
                } else if (has_system) {
                    selected = &system;
                } else if (has_music) {
                    selected = &music;
                } else if (has_spotify) {
                    selected = &spotify;
                }

                if (!selected) {
                    // No session is normal (nothing is publishing). Only surface
                    // a real scripting failure when every source failed.
                    if (sb_failure && !has_system) {
                        media_error(env, sb_failure.UTF8String);
                    }
                    return nullptr;
                }

                if (client->active_source != selected->source) {
                    client->artwork_key.clear();
                    client->has_artwork = false;
                }
                client->active_source = selected->source;

                std::vector<uint8_t> artwork = selected->artwork;
                if (artwork.empty() && selected->source == u"com.apple.Music") {
                    read_music_artwork(client, *selected, artwork);
                }
                if (selected == &system) {
                    client->last_system_track = selected->track;
                    client->last_system_artwork = artwork;
                }

                return media_snapshot(
                    env, selected->source, selected->track, selected->title,
                    selected->artist, std::max<int64_t>(0, selected->position),
                    std::max<int64_t>(0, selected->duration), selected->playing,
                    selected->can_seek, selected->speed, 0, artwork,
                    selected->artwork_url, selected->sampled_at);
            } @catch (NSException* exception) {
                report(env, exception);
                return nullptr;
            }
        }
    });
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_seek(
    JNIEnv* env, jobject, jlong handle, jstring source, jstring track,
    jlong position) {
    return guarded_media<jboolean>(env, false, [&]() -> jboolean {
        @autoreleasepool {
            @try {
                auto* client = reinterpret_cast<MediaClient*>(handle);
                const auto source_id = native_string(env, source);
                const auto track_id_value = native_string(env, track);
                auto app = matching_scripted_player(client, source_id, track_id_value);
                if (app) {
                    app.playerPosition = std::max<jlong>(0, position) / 1000.0;
                    if (client->errors.lastError) {
                        media_error(env,
                                    client->errors.lastError.localizedDescription.UTF8String);
                        return false;
                    }
                    return true;
                }
                // System-sourced snapshots carry MediaRemote track IDs; accept
                // them when the last published system track is still the target.
                if (system_track_matches(client, track_id_value) && mr::set_elapsed) {
                    mr::set_elapsed(std::max<jlong>(0, position) / 1000.0);
                    return true;
                }
                return false;
            } @catch (NSException* exception) {
                report(env, exception);
                return false;
            }
        }
    });
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_toggle(
    JNIEnv* env, jobject, jlong handle, jstring source, jstring track) {
    return guarded_media<jboolean>(env, false, [&]() -> jboolean {
        @autoreleasepool {
            @try {
                auto* client = reinterpret_cast<MediaClient*>(handle);
                const auto source_id = native_string(env, source);
                const auto track_id_value = native_string(env, track);
                auto app = matching_scripted_player(client, source_id, track_id_value);
                if (app) {
                    [app playpause];
                    if (client->errors.lastError) {
                        media_error(env,
                                    client->errors.lastError.localizedDescription.UTF8String);
                        return false;
                    }
                    return true;
                }
                if (system_track_matches(client, track_id_value) && mr::send_command) {
                    return mr::send_command(mr::kTogglePlayPause, nil) ? true : false;
                }
                return false;
            } @catch (NSException* exception) {
                report(env, exception);
                return false;
            }
        }
    });
}

extern "C" JNIEXPORT void JNICALL
Java_com_mocharealm_accompanist_sample_desktop_NativeMediaBridge_release(
    JNIEnv*, jobject, jlong handle) {
    @autoreleasepool { delete reinterpret_cast<MediaClient*>(handle); }
}
