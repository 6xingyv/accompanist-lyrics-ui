// `SystemFont` is a complete descriptor; some fields (weight/italic/locale/index)
// aren't consumed yet — they're kept for weight-aware fallback and the planned
// `AFontMatcher` path — so silence dead-code until then.
#![allow(dead_code)]

//! Native enumeration of the platform font collection via the NDK
//! `ASystemFontIterator` API (available since Android API 29; the app's minSdk is
//! 29, so the symbols are always present and can be linked directly).
//!
//! Each entry exposes the on-disk path, TTC collection index, weight, slant and
//! locale. We feed these into cosmic-text's font database so its built-in,
//! locale/attribute-aware fallback can resolve any glyph the user's font chain
//! doesn't cover — instead of relying on a hand-maintained list of "Noto …"
//! family-name guesses.

use libc::{c_char, c_uint};
use std::ffi::{CStr, CString};

// Opaque NDK handles (see <android/font.h> / <android/system_fonts.h> /
// <android/font_matcher.h>).
#[repr(C)]
struct ASystemFontIterator {
    _private: [u8; 0],
}
#[repr(C)]
struct AFont {
    _private: [u8; 0],
}
#[repr(C)]
struct AFontMatcher {
    _private: [u8; 0],
}

#[link(name = "android")]
extern "C" {
    fn ASystemFontIterator_open() -> *mut ASystemFontIterator;
    fn ASystemFontIterator_next(iterator: *mut ASystemFontIterator) -> *mut AFont;
    fn ASystemFontIterator_close(iterator: *mut ASystemFontIterator);

    fn AFont_close(font: *mut AFont);
    fn AFont_getFontFilePath(font: *const AFont) -> *const c_char;
    fn AFont_getWeight(font: *const AFont) -> u16;
    fn AFont_isItalic(font: *const AFont) -> bool;
    fn AFont_getLocale(font: *const AFont) -> *const c_char;
    fn AFont_getCollectionIndex(font: *const AFont) -> libc::size_t;

    fn AFontMatcher_create() -> *mut AFontMatcher;
    fn AFontMatcher_destroy(matcher: *mut AFontMatcher);
    fn AFontMatcher_setStyle(matcher: *mut AFontMatcher, weight: u16, italic: bool);
    fn AFontMatcher_setLocales(matcher: *mut AFontMatcher, language_tags: *const c_char);
    fn AFontMatcher_match(
        matcher: *mut AFontMatcher,
        family_name: *const c_char,
        text: *const u16,
        text_length: c_uint,
        run_length_out: *mut c_uint,
    ) -> *mut AFont;
}

/// One face reported by the platform font collection.
#[derive(Debug, Clone)]
pub struct SystemFont {
    pub path: String,
    pub collection_index: u32,
    pub weight: u16,
    pub italic: bool,
    /// BCP-47-ish locale list the face is tagged for (e.g. `zh-Hans`), if any.
    pub locale: Option<String>,
}

/// Enumerate every face in the platform font collection. Returns an empty vector
/// if the iterator can't be opened. Safe to call from any thread.
pub fn enumerate_system_fonts() -> Vec<SystemFont> {
    let mut fonts = Vec::new();

    // SAFETY: the NDK font iterator is a simple open/next/close C API. We never
    // hold an `AFont`/iterator pointer past its documented lifetime: each `AFont`
    // is read and closed within the loop body, and the iterator is closed before
    // returning. All returned strings are copied out before the source is freed.
    unsafe {
        let iterator = ASystemFontIterator_open();
        if iterator.is_null() {
            return fonts;
        }

        loop {
            let font = ASystemFontIterator_next(iterator);
            if font.is_null() {
                break;
            }

            let path_ptr = AFont_getFontFilePath(font);
            if let Some(path) = ptr_to_string(path_ptr) {
                fonts.push(SystemFont {
                    path,
                    collection_index: AFont_getCollectionIndex(font) as u32,
                    weight: AFont_getWeight(font),
                    italic: AFont_isItalic(font),
                    locale: ptr_to_string(AFont_getLocale(font)),
                });
            }

            AFont_close(font);
        }

        ASystemFontIterator_close(iterator);
    }

    fonts
}

/// Copy a borrowed C string into an owned `String`, or `None` if null/invalid.
unsafe fn ptr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(str::to_owned)
}

/// Safe wrapper over the NDK `AFontMatcher` — Android's own on-demand font
/// fallback. Given a character it returns the system font that should render it,
/// so a scene can lazily load only the few fonts it actually needs instead of the
/// whole ~200-file collection up front.
pub struct FontMatcher {
    matcher: *mut AFontMatcher,
}

impl FontMatcher {
    pub fn new() -> Option<Self> {
        // SAFETY: create returns a new owned matcher or null.
        let matcher = unsafe { AFontMatcher_create() };
        if matcher.is_null() {
            None
        } else {
            Some(Self { matcher })
        }
    }

    pub fn set_style(&mut self, weight: u16, italic: bool) {
        unsafe { AFontMatcher_setStyle(self.matcher, weight, italic) };
    }

    pub fn set_locales(&mut self, language_tags: &str) {
        if let Ok(tags) = CString::new(language_tags) {
            unsafe { AFontMatcher_setLocales(self.matcher, tags.as_ptr()) };
        }
    }

    /// Find the system font Android would use to render `ch` (starting from
    /// `family`, falling back as needed). Returns its file path + TTC index.
    pub fn match_char(&self, ch: char, family: Option<&str>) -> Option<SystemFont> {
        let mut utf16 = [0u16; 2];
        let encoded = ch.encode_utf16(&mut utf16);
        let family_c = family.and_then(|f| CString::new(f).ok());
        let family_ptr = family_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());

        let mut run_length: c_uint = 0;
        // SAFETY: `self.matcher` is a live matcher; `encoded` outlives the call;
        // the returned `AFont` is read out and closed before returning.
        unsafe {
            let font = AFontMatcher_match(
                self.matcher,
                family_ptr,
                encoded.as_ptr(),
                encoded.len() as c_uint,
                &mut run_length,
            );
            if font.is_null() {
                return None;
            }
            let result = ptr_to_string(AFont_getFontFilePath(font)).map(|path| SystemFont {
                path,
                collection_index: AFont_getCollectionIndex(font) as u32,
                weight: AFont_getWeight(font),
                italic: AFont_isItalic(font),
                locale: ptr_to_string(AFont_getLocale(font)),
            });
            AFont_close(font);
            result
        }
    }
}

impl Drop for FontMatcher {
    fn drop(&mut self) {
        // SAFETY: matcher was created by `AFontMatcher_create` and not yet freed.
        unsafe { AFontMatcher_destroy(self.matcher) };
    }
}

// The matcher is only ever accessed while the engine's `Mutex` is held, so the
// handle is never used from two threads at once — safe to move between threads.
unsafe impl Send for FontMatcher {}

impl std::fmt::Debug for FontMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FontMatcher(<AFontMatcher>)")
    }
}
