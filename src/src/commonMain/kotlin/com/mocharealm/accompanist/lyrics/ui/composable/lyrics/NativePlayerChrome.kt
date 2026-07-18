package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

/**
 * Complete Rust-rendered portrait player chrome.
 *
 * When supplied to [KaraokeLyricsView], the native surface owns the three-page
 * player (lyrics / artwork / queue) plus transport, progress, and mode nav. This
 * is mutually exclusive with the legacy top-bar-only mode driven by bare
 * `title` / `artist` / `onControlsClick`.
 *
 * **Owned by Rust (do not drive from Compose every frame):**
 * - page selection: resting **artwork** page; lyrics / queue are toggles;
 *   output never changes the page
 * - position, duration, and play/pause when music-foundation's native clock is
 *   available (`useMusicFoundationClock = true`)
 *
 * Host only pushes **metadata** here: title/artist, liked, queue list/filter
 * labels. [durationMs] / [isPlaying] remain as fallbacks for hosts without a
 * process-local music engine (e.g. the sample app).
 */
data class NativePlayerChrome(
    val title: String,
    val artist: String = "",
    /** Fallback duration when music-foundation is not present. */
    val durationMs: Int = 0,
    /** Fallback play state when music-foundation is not present. */
    val isPlaying: Boolean = false,
    val liked: Boolean = false,
    val presentation: NativePlayerPresentation = NativePlayerPresentation.Full,
    /** Logical mini-player viewport inside a fixed-size native surface, in px. */
    val viewportWidth: Float? = null,
    val viewportHeight: Float? = null,
    /**
     * Initial page only (default: full artwork with no mode chip selected).
     * After the first scene, Rust owns lyrics/queue toggles; subsequent host
     * updates must not fight that state.
     */
    val initialScreen: NativePlayerScreen = NativePlayerScreen.Artwork,
    val queueTitle: String = "",
    val queueSource: String = "",
    val queueFilter: NativeQueueFilter = NativeQueueFilter.UpNext,
    val queueItems: List<NativePlayerQueueItem> = emptyList(),
)

/** Fixed-surface geometry used by the Android host for mini/full transitions. */
data class NativePlayerExpansionGeometry(
    val collapsedLeft: Float,
    val collapsedTop: Float,
    val collapsedWidth: Float,
    val collapsedHeight: Float,
    val collapsedRadius: Float,
    val expandedTopLeftRadius: Float = 0f,
    val expandedTopRightRadius: Float = 0f,
    val expandedBottomRightRadius: Float = 0f,
    val expandedBottomLeftRadius: Float = 0f,
)

enum class NativePlayerPresentation(val wireValue: String) {
    Mini("mini"),
    Full("full"),
}

/** Which of the three portrait player pages is currently settled / targeted. */
enum class NativePlayerScreen(val wireValue: String) {
    Lyrics("lyrics"),
    Artwork("artwork"),
    Queue("queue"),
}

/** Queue page filter chips. Wire values match the Rust serde enum. */
enum class NativeQueueFilter(val wireValue: String) {
    UpNext("upNext"),
    Shuffle("shuffle"),
    RepeatOne("repeatOne"),
    Album("album"),
}

data class NativePlayerQueueItem(
    val title: String,
    val artist: String = "",
    val artworkUri: String? = null,
)

/**
 * Stable native player action codes emitted by the Rust hit-tester.
 * Values must stay aligned with `PlayerButton` in the lyrics-renderer crate.
 *
 * [Lyrics] / [Queue] toggle the corresponding page in Rust (second tap returns
 * to artwork). [Output] does not change the page — hosts should only open a
 * media sheet.
 */
enum class NativePlayerAction(val code: Int) {
    Favorite(1),
    More(2),
    Previous(3),
    PlayPause(4),
    Next(5),
    Lyrics(6),
    Output(7),
    Queue(8),
    QueueUpNext(9),
    QueueShuffle(10),
    QueueRepeatOne(11),
    QueueAlbum(12),
    Open(13),
    ;

    companion object {
        private val byCode = entries.associateBy { it.code }

        fun fromCode(code: Int): NativePlayerAction? = byCode[code]
    }
}
