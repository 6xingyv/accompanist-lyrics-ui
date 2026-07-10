//! Font subsystem: loading user/system faces into cosmic-text's db, Skia
//! typeface resolution & caching, NDK `AFontMatcher` lazy fallback, and
//! per-cluster family selection for the font tower. Split out of `renderer.rs`.

use super::*;

pub(super) fn match_skia_typeface_for_face(face: &fontdb::FaceInfo) -> Option<Typeface> {
    let style = FontStyle::new(
        font_style::Weight::from(face.weight.0 as i32),
        font_style::Width::NORMAL,
        match face.style {
            fontdb::Style::Normal => font_style::Slant::Upright,
            fontdb::Style::Italic => font_style::Slant::Italic,
            fontdb::Style::Oblique => font_style::Slant::Oblique,
        },
    );

    with_skia_font_mgr(|font_mgr| {
        face.families
            .iter()
            .map(|(name, _)| name.as_str())
            .chain(std::iter::once(face.post_script_name.as_str()))
            .filter(|name| !name.is_empty())
            .find_map(|name| font_mgr.match_family_style(name, style))
    })
}

pub(super) fn with_skia_font_mgr<R>(f: impl FnOnce(&FontMgr) -> R) -> R {
    thread_local! {
        static FONT_MGR: std::cell::RefCell<Option<FontMgr>> = std::cell::RefCell::new(None);
    }

    FONT_MGR.with(|cell| {
        let needs_init = cell.borrow().is_none();
        if needs_init {
            *cell.borrow_mut() = Some(FontMgr::new());
        }
        let font_mgr = cell.borrow();
        f(font_mgr
            .as_ref()
            .expect("thread-local Skia FontMgr must be initialized"))
    })
}

pub(super) fn skia_typeface_from_path(path: &std::path::Path, face_index: u32) -> Option<Typeface> {
    let data = Data::from_filename(path)?;
    skia_typeface_from_data(data, face_index)
}

pub(super) fn skia_typeface_from_bytes(bytes: &[u8], face_index: u32) -> Option<Typeface> {
    let data = Data::new_copy(bytes);
    skia_typeface_from_data(data, face_index)
}

pub(super) fn skia_typeface_from_data(data: Data, face_index: u32) -> Option<Typeface> {
    FontMgr::new().new_from_data(data.as_bytes(), face_index as usize)
}

pub(super) fn skia_typeface_from_face_source(face: &fontdb::FaceInfo) -> Option<Typeface> {
    match &face.source {
        fontdb::Source::Binary(bytes) => {
            skia_typeface_from_bytes(bytes.as_ref().as_ref(), face.index)
        }
        fontdb::Source::File(path) => skia_typeface_from_path(path, face.index),
        fontdb::Source::SharedFile(path, _) => skia_typeface_from_path(path, face.index),
    }
}

pub(super) fn collect_text_font_usage(
    text: &PreparedText,
    font_ids: &mut Vec<fontdb::ID>,
) -> usize {
    let mut glyph_count = 0;
    for row in &text.rows {
        for glyph in &row.glyphs {
            glyph_count += 1;
            let font_id = glyph.physical.cache_key.font_id;
            if !font_ids.contains(&font_id) {
                font_ids.push(font_id);
            }
        }
    }
    glyph_count
}

pub(super) fn count_text_missing_typeface_glyphs(
    text: &PreparedText,
    typefaces: &HashMap<fontdb::ID, Typeface>,
) -> usize {
    text.rows
        .iter()
        .flat_map(|row| row.glyphs.iter())
        .filter(|glyph| !typefaces.contains_key(&glyph.physical.cache_key.font_id))
        .count()
}

pub(super) fn describe_font_face(id: fontdb::ID, face: &fontdb::FaceInfo) -> String {
    let family = face
        .families
        .first()
        .map(|(name, _)| name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(face.post_script_name.as_str());
    let source = match &face.source {
        fontdb::Source::Binary(_) => "binary".to_string(),
        fontdb::Source::File(path) => path.display().to_string(),
        fontdb::Source::SharedFile(path, _) => path.display().to_string(),
    };
    format!(
        "id={:?} family={} index={} weight={} style={:?} source={}",
        id, family, face.index, face.weight.0, face.style, source
    )
}

#[cfg(target_os = "android")]
pub(super) fn should_read_font_path_for_skia(path: &str) -> bool {
    let path = path.replace('\\', "/");
    !(path.starts_with("/system/fonts/")
        || path.starts_with("/apex/")
        || path.starts_with("/product/fonts/")
        || path.starts_with("/vendor/fonts/"))
}

#[cfg(not(target_os = "android"))]
pub(super) fn should_read_font_path_for_skia(_path: &str) -> bool {
    true
}

impl LyricsRenderer {
    pub fn load_font_bytes(&mut self, bytes: Vec<u8>, face_index: u32) {
        let skia_typeface = skia_typeface_from_bytes(bytes.as_slice(), face_index);
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(fontdb::Source::Binary(Arc::new(bytes)));
        self.register_loaded_face(ids.as_slice(), face_index, skia_typeface);
        self.font_selection_cache.clear();
        self.reset_layout_animation_state();
        self.reset_manual_scroll();
        self.scene = None;
    }

    pub fn load_font_path(&mut self, path: &str, face_index: u32) -> bool {
        if !std::path::Path::new(path).exists() {
            return false;
        }
        let skia_typeface = if should_read_font_path_for_skia(path) {
            skia_typeface_from_path(std::path::Path::new(path), face_index)
        } else {
            None
        };
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(fontdb::Source::File(std::path::PathBuf::from(path)));
        self.register_loaded_face(ids.as_slice(), face_index, skia_typeface);
        self.font_selection_cache.clear();
        self.reset_layout_animation_state();
        self.reset_manual_scroll();
        self.scene = None;
        true
    }

    /// Populate cosmic-text's font database with every face in the platform font
    /// collection (enumerated natively via the NDK `ASystemFontIterator`), so its
    /// built-in locale/attribute-aware fallback can resolve glyphs the user's
    /// font chain doesn't cover. These go into the db only — not the user
    /// `font_stack` — so the user's loaded fonts keep priority; the system fonts
    /// are purely the fallback pool. Returns the number of font files loaded.
    #[cfg(target_os = "android")]
    pub fn load_system_fonts(&mut self) -> usize {
        let fonts = crate::system_fonts::enumerate_system_fonts();
        let mut seen_paths = std::collections::HashSet::new();
        let mut loaded = 0usize;
        for font in &fonts {
            // A TTC reports one entry per face but `load_font_source` ingests the
            // whole file at once, so load each unique path only once.
            if !seen_paths.insert(font.path.as_str()) {
                continue;
            }
            if std::path::Path::new(&font.path).exists() {
                self.font_system
                    .db_mut()
                    .load_font_source(fontdb::Source::File(std::path::PathBuf::from(&font.path)));
                loaded += 1;
            }
        }
        info!(
            "[LyricsRenderer] loaded {} system font files ({} faces enumerated) into fallback pool",
            loaded,
            fonts.len()
        );
        self.font_selection_cache.clear();
        loaded
    }

    #[cfg(not(target_os = "android"))]
    pub fn load_system_fonts(&mut self) -> usize {
        let before = self.font_system.db().len();
        self.font_system.db_mut().load_system_fonts();
        let after = self.font_system.db().len();
        self.font_selection_cache.clear();
        after.saturating_sub(before)
    }

    /// Lazily pull in the system fonts a piece of text needs (Android): for each
    /// glyph not yet seen at this weight/style, ask the NDK `AFontMatcher` which
    /// system font covers it and load just that file into the db. Cached per
    /// (char, weight, italic) so each glyph is matched at most once.
    #[cfg(target_os = "android")]
    pub(super) fn ensure_fonts_for_text(&mut self, text: &str, attrs: TextAttrs) {
        if self.font_matcher.is_none() {
            return;
        }

        let mut to_match: Vec<char> = Vec::new();
        for ch in text.chars() {
            if ch.is_whitespace() || ch.is_control() {
                continue;
            }
            if self.matched_glyphs.insert((ch, attrs.weight, attrs.italic)) {
                to_match.push(ch);
            }
        }
        if to_match.is_empty() {
            return;
        }

        // Ask the NDK matcher which system font renders each new glyph. Query
        // from the canonical default family ("sans-serif") — NOT the user's
        // primary family. `AFontMatcher` only knows *system* families; handing it
        // a custom/app font name (which it can't resolve) makes it mis-resolve the
        // fallback, so CJK/emoji glyphs the app font lacks stop falling back once a
        // custom `FontResource` is set. Flutter behaves the same way: its Android
        // system fallback manager (`SkFontMgr_android`) is queried independently of
        // the app's own font. The primary font's own coverage is decided separately
        // in `select_family_for_cluster`, so it still wins for glyphs it has.
        let mut matched: Vec<(char, crate::system_fonts::SystemFont)> = Vec::new();
        if let Some(matcher) = self.font_matcher.as_mut() {
            matcher.set_style(attrs.weight, attrs.italic);
            for ch in to_match {
                if let Some(font) = matcher.match_char(ch, Some("sans-serif")) {
                    matched.push((ch, font));
                }
            }
        }

        let mut loaded_any = false;
        for (ch, font) in matched {
            // Load the whole TTC once.
            if self.loaded_system_paths.insert(font.path.clone())
                && std::path::Path::new(&font.path).exists()
            {
                self.font_system
                    .db_mut()
                    .load_font_source(fontdb::Source::File(std::path::PathBuf::from(&font.path)));
                loaded_any = true;
            }
            // Map the glyph to the family of the exact face the matcher chose (its
            // collection index within the TTC). The matcher already confirmed this
            // face covers the glyph, so this is authoritative.
            if let Some(family_name) = self.family_for_source(&font.path, font.collection_index) {
                self.matched_char_family.insert(ch, family_name);
            }
        }
        if loaded_any {
            self.font_selection_cache.clear();
        }
    }

    /// Family name of the db face loaded from `path` at TTC `collection_index`.
    #[cfg(target_os = "android")]
    fn family_for_source(&self, path: &str, collection_index: u32) -> Option<String> {
        self.font_system
            .db()
            .faces()
            .find(|face| {
                face.index == collection_index
                    && match &face.source {
                        fontdb::Source::File(p) => p.to_str() == Some(path),
                        fontdb::Source::SharedFile(p, _) => p.to_str() == Some(path),
                        fontdb::Source::Binary(_) => false,
                    }
            })
            .and_then(|face| {
                face.families
                    .first()
                    .map(|(name, _)| name.clone())
                    .filter(|name| !name.is_empty())
                    .or_else(|| Some(face.post_script_name.clone()).filter(|name| !name.is_empty()))
            })
    }

    pub(super) fn register_loaded_face(
        &mut self,
        ids: &[fontdb::ID],
        face_index: u32,
        skia_typeface: Option<Typeface>,
    ) {
        let selected_id = ids
            .iter()
            .copied()
            .find(|id| {
                self.font_system
                    .db()
                    .face(*id)
                    .is_some_and(|face| face.index == face_index)
            })
            .or_else(|| ids.first().copied());

        let Some(id) = selected_id else {
            return;
        };

        let Some((family_name, typeface)) = self.font_system.db().face(id).map(|face| {
            let family_name = face
                .families
                .first()
                .map(|(name, _)| name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| face.post_script_name.clone());
            let typeface = skia_typeface.or_else(|| match_skia_typeface_for_face(face));
            (family_name, typeface)
        }) else {
            return;
        };

        if family_name.is_empty() {
            return;
        }

        self.font_stack.push(RendererFontFace { id, family_name });
        if let Some(typeface) = typeface {
            self.skia_typefaces.insert(id, typeface);
        }
    }

    pub(super) fn ensure_skia_typefaces_for_scene(&mut self) -> TypefaceEnsureStats {
        // The scene's glyph→font mapping is fixed once installed, so skip the
        // whole-scene resolve scan on every frame and only run it when a new
        // scene marked the cache dirty. In steady state this turns a per-frame
        // O(all glyphs) walk into a single field check.
        if !self.skia_typefaces_dirty {
            let resolved = self.skia_typefaces.len();
            return TypefaceEnsureStats {
                typefaces_before: resolved,
                typefaces_after: resolved,
                ..TypefaceEnsureStats::default()
            };
        }
        let Some(scene) = &self.scene else {
            return TypefaceEnsureStats::default();
        };

        let mut scene_font_ids = Vec::new();
        let mut scene_glyphs = 0;
        for line in &scene.lines {
            match &line.kind {
                PreparedLineKind::Karaoke { text, .. } => {
                    scene_glyphs += collect_text_font_usage(text, &mut scene_font_ids);
                }
                PreparedLineKind::Synced { text } => {
                    scene_glyphs += collect_text_font_usage(text, &mut scene_font_ids);
                }
            }
            if let Some(translation) = &line.translation {
                scene_glyphs += collect_text_font_usage(translation, &mut scene_font_ids);
            }
            if let Some(phonetic) = &line.phonetic {
                scene_glyphs += collect_text_font_usage(phonetic, &mut scene_font_ids);
            }
        }
        // The top-bar title/artist share the same Skia typeface pool, so their font
        // ids must be resolved too or the text would draw with missing typefaces.
        if let Some(top_bar) = &scene.top_bar {
            scene_glyphs += collect_text_font_usage(&top_bar.title, &mut scene_font_ids);
            scene_glyphs += collect_text_font_usage(&top_bar.artist, &mut scene_font_ids);
        }

        let missing_ids: Vec<fontdb::ID> = scene_font_ids
            .iter()
            .copied()
            .filter(|id| !self.skia_typefaces.contains_key(id))
            .collect();
        let mut stats = TypefaceEnsureStats {
            scene_glyphs,
            scene_font_ids: scene_font_ids.len(),
            typefaces_before: self.skia_typefaces.len(),
            missing_before: missing_ids.len(),
            ..TypefaceEnsureStats::default()
        };

        for id in missing_ids {
            if self.skia_typefaces.contains_key(&id) {
                continue;
            }

            let Some(face) = self.font_system.db().face(id) else {
                stats
                    .failed_faces
                    .push(format!("id={:?} face=<missing>", id));
                continue;
            };

            // Load the typeface from the face's own source first so a variable
            // font keeps its axes (the drawn glyphs are later instanced at the
            // requested `wght` in `draw.rs`) and the exact concrete face is used.
            // Fall back to the system FontMgr only if the source can't be read.
            let typeface =
                skia_typeface_from_face_source(face).or_else(|| match_skia_typeface_for_face(face));

            match typeface {
                Some(tf) => {
                    self.skia_typefaces.insert(id, tf);
                    stats.loaded_from_source += 1;
                }
                None => stats.failed_faces.push(describe_font_face(id, face)),
            }
        }

        stats.typefaces_after = self.skia_typefaces.len();
        stats.missing_after = scene_font_ids
            .iter()
            .filter(|id| !self.skia_typefaces.contains_key(id))
            .count();
        // Any face that still can't be resolved has no source we can load, so it
        // won't resolve on a later frame either — clear the dirty flag regardless
        // to avoid re-scanning the whole scene every frame chasing it.
        self.skia_typefaces_dirty = false;
        stats
    }

    pub(super) fn build_font_spans<'a>(
        &mut self,
        spans: impl Iterator<Item = (&'a str, usize)>,
        fallback_text: &str,
    ) -> Vec<FontTextSpan> {
        let mut result = Vec::new();
        for (text, metadata) in spans {
            self.push_font_spans_for_text(&mut result, text, metadata);
        }

        if result.is_empty() && !fallback_text.is_empty() {
            self.push_font_spans_for_text(&mut result, fallback_text, 0);
        }
        result
    }

    pub(super) fn push_font_spans_for_text(
        &mut self,
        result: &mut Vec<FontTextSpan>,
        text: &str,
        metadata: usize,
    ) {
        for cluster in UnicodeSegmentation::graphemes(text, true) {
            let family_name = self.select_family_for_cluster(cluster);
            if let Some(last) = result.last_mut() {
                if last.metadata == metadata && last.family_name == family_name {
                    last.text.push_str(cluster);
                    continue;
                }
            }

            result.push(FontTextSpan {
                text: cluster.to_string(),
                metadata,
                family_name,
            });
        }
    }

    pub(super) fn select_family_for_cluster(&mut self, cluster: &str) -> Option<String> {
        if let Some(cached) = self.font_selection_cache.get(cluster) {
            return cached.clone();
        }
        let selected = self.select_family_for_cluster_uncached(cluster);
        self.font_selection_cache
            .insert(cluster.to_string(), selected.clone());
        selected
    }

    pub(super) fn select_family_for_cluster_uncached(&mut self, cluster: &str) -> Option<String> {
        // The user's loaded font leads the tower: font_stack[0] is the primary
        // (custom) font, and if it can render this cluster it wins outright —
        // including for CJK, where the old code let hard-coded "Noto …" system
        // families outrank it. System fonts only fill what the primary can't.
        let primary_id = self.font_stack.first().map(|face| face.id);
        if let Some(id) = primary_id {
            if self.font_supports_cluster(id, cluster) {
                return self.font_stack.first().map(|face| face.family_name.clone());
            }
        }

        if contains_han(cluster) {
            let mut selected: Option<(usize, usize)> = None;
            for index in 0..self.font_stack.len() {
                let id = self.font_stack[index].id;
                if !self.font_supports_cluster(id, cluster) {
                    continue;
                }
                let priority =
                    cjk_family_priority(&self.font_stack[index].family_name, &self.locale);
                match selected {
                    Some((best_priority, best_index))
                        if best_priority < priority
                            || (best_priority == priority && best_index <= index) => {}
                    _ => selected = Some((priority, index)),
                }
            }

            if let Some((_, index)) = selected {
                return Some(self.font_stack[index].family_name.clone());
            }
        }

        for index in 0..self.font_stack.len() {
            let id = self.font_stack[index].id;
            if self.font_supports_cluster(id, cluster) {
                return Some(self.font_stack[index].family_name.clone());
            }
        }

        // Nothing in the user chain covers this cluster (e.g. CJK, or '…' with a
        // Latin-only custom font). Use the system font the NDK matcher already
        // resolved for this glyph — MiSans on Xiaomi, Noto elsewhere — so cosmic
        // -text shapes with a family that actually has it, instead of one that
        // doesn't and dropping to its hard-coded Roboto/Droid preset fallback.
        #[cfg(target_os = "android")]
        if let Some(ch) = cluster
            .chars()
            .find(|c| !c.is_whitespace() && !c.is_control())
        {
            if let Some(family) = self.matched_char_family.get(&ch) {
                return Some(family.clone());
            }
        }

        self.font_stack.first().map(|face| face.family_name.clone())
    }

    pub(super) fn font_supports_cluster(&mut self, id: fontdb::ID, cluster: &str) -> bool {
        // Prefer Skia's cmap coverage — it is authoritative (it's the same
        // coverage source Flutter's whole text stack uses). cosmic-text's
        // `get_font_supported_codepoints_in_word` reports **0** supported
        // codepoints for some perfectly valid fonts — the variable "SF Pro" (even
        // for plain Latin) and CJK OTC/CFF collection faces — which made the engine
        // reject the user's own `FontResource` and fall *everything* back to system
        // fonts (so a custom font appeared to have no effect). Every face in the
        // user `font_stack` has a Skia typeface resolved in `register_loaded_face`,
        // so this path is taken for exactly the faces this function is asked about.
        if let Some(typeface) = self.skia_typefaces.get(&id) {
            for ch in cluster.chars() {
                if ch.is_control() {
                    continue;
                }
                if typeface.unichar_to_glyph(ch as i32) == 0 {
                    return false;
                }
            }
            return true;
        }

        // No Skia typeface for this face (shouldn't happen for font_stack faces):
        // fall back to cosmic-text's probe.
        let expected = cluster.chars().filter(|ch| !ch.is_control()).count();
        if expected == 0 {
            return true;
        }
        self.font_system
            .get_font_supported_codepoints_in_word(id, fontdb::Weight::NORMAL, cluster)
            .is_some_and(|count| count >= expected)
    }
}
