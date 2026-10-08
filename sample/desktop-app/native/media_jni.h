#pragma once
#include <jni.h>
#include <cstdint>
#include <string>
#include <vector>
#include <exception>

// Keep in sync with NativeMediaSnapshot's constructor. Strings use UTF-16 so
// non-BMP titles and embedded separators survive JNI without modified-UTF8 loss.
inline jstring java_string(JNIEnv* env, const std::u16string& value) {
    return env->NewString(reinterpret_cast<const jchar*>(value.data()), static_cast<jsize>(value.size()));
}
inline std::u16string native_string(JNIEnv* env, jstring value) {
    if (!value) return {};
    const auto length = env->GetStringLength(value);
    const auto* chars = env->GetStringChars(value, nullptr);
    if (!chars) return {};
    std::u16string result(reinterpret_cast<const char16_t*>(chars), length);
    env->ReleaseStringChars(value, chars);
    return result;
}
inline void media_error(JNIEnv* env, const char* message) {
    if (!env->ExceptionCheck()) {
        const auto type = env->FindClass("java/lang/IllegalStateException");
        if (type) env->ThrowNew(type, message);
    }
}
inline jlong java_nano_time(JNIEnv* env) {
    const auto type = env->FindClass("java/lang/System");
    if (!type) return 0;
    const auto method = env->GetStaticMethodID(type, "nanoTime", "()J");
    const auto time = method ? env->CallStaticLongMethod(type, method) : 0;
    env->DeleteLocalRef(type);
    return time;
}
template<typename T, typename Action>
inline T guarded_media(JNIEnv* env, T fallback, Action action) {
    try { return action(); }
    catch (const std::exception& failure) { media_error(env, failure.what()); }
    catch (...) { media_error(env, "Native media operation failed"); }
    return fallback;
}
inline jobject media_snapshot(JNIEnv* env,
    const std::u16string& source, const std::u16string& track,
    const std::u16string& title, const std::u16string& artist,
    int64_t position, int64_t duration, bool playing, bool can_seek,
    float speed, int64_t token, const std::vector<uint8_t>& artwork,
    const std::u16string& artwork_url, jlong sampled_at) {
    const auto type = env->FindClass("com/mocharealm/accompanist/sample/desktop/NativeMediaSnapshot");
    if (!type) return nullptr;
    const auto constructor = env->GetMethodID(type, "<init>",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;JJZZFJ[BLjava/lang/String;J)V");
    if (!constructor) return nullptr;
    jbyteArray image = nullptr;
    if (!artwork.empty()) {
        image = env->NewByteArray(static_cast<jsize>(artwork.size()));
        if (!image) return nullptr;
        env->SetByteArrayRegion(image, 0, static_cast<jsize>(artwork.size()),
            reinterpret_cast<const jbyte*>(artwork.data()));
    }
    auto source_string = java_string(env, source);
    auto track_string = java_string(env, track);
    auto title_string = java_string(env, title);
    auto artist_string = java_string(env, artist);
    auto url_string = artwork_url.empty() ? nullptr : java_string(env, artwork_url);
    if (env->ExceptionCheck()) return nullptr;
    return env->NewObject(type, constructor, source_string, track_string, title_string, artist_string,
        static_cast<jlong>(position), static_cast<jlong>(duration),
        static_cast<jboolean>(playing), static_cast<jboolean>(can_seek),
        static_cast<jfloat>(speed), static_cast<jlong>(token), image, url_string, sampled_at);
}
