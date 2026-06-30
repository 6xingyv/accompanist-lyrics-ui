package com.mocharealm.accompanist.lyrics.text

/**
 * Describes a font source for the native text engine.
 *
 * A source should provide [bytes], [path], [assetPath], or [resourceId]. Android
 * system and bundled fonts should prefer path/asset/resource descriptors so
 * large TTC/TTF files can be mmap'ed without copying the whole file into Kotlin
 * heap memory.
 */
data class NativeFontSource(
    val bytes: ByteArray? = null,
    val path: String? = null,
    val assetPath: String? = null,
    val resourceId: Int? = null,
    val ttcIndex: Int = 0,
    val weight: Int? = null,
    val italic: Boolean = false,
    val axes: List<NativeFontAxis> = emptyList(),
    val languageTags: List<String> = emptyList(),
    val fallbackFor: String? = null,
    val sourceId: String? = null,
    internal val platformContext: Any? = null
) {
    init {
        require(
            bytes != null ||
                !path.isNullOrBlank() ||
                !assetPath.isNullOrBlank() ||
                resourceId != null
        ) {
            "NativeFontSource requires bytes, path, assetPath, or resourceId"
        }
        require(ttcIndex >= 0) {
            "ttcIndex must be >= 0"
        }
    }

    fun stableKey(): String {
        val byteKey = bytes?.contentHashCode()?.toString(16) ?: "no-bytes"
        val pathKey = path ?: "no-path"
        val assetKey = assetPath ?: "no-asset"
        val resourceKey = resourceId?.toString() ?: "no-resource"
        val axisKey = axes.joinToString(";") { "${it.tag}:${it.value}" }
        val langKey = languageTags.joinToString(",")
        return listOf(
            sourceId,
            pathKey,
            assetKey,
            resourceKey,
            byteKey,
            ttcIndex,
            weight,
            italic,
            axisKey,
            langKey,
            fallbackFor
        )
            .joinToString("|")
    }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is NativeFontSource) return false
        return bytesEqual(bytes, other.bytes) &&
            path == other.path &&
            assetPath == other.assetPath &&
            resourceId == other.resourceId &&
            ttcIndex == other.ttcIndex &&
            weight == other.weight &&
            italic == other.italic &&
            axes == other.axes &&
            languageTags == other.languageTags &&
            fallbackFor == other.fallbackFor &&
            sourceId == other.sourceId
    }

    override fun hashCode(): Int {
        var result = bytes?.contentHashCode() ?: 0
        result = 31 * result + (path?.hashCode() ?: 0)
        result = 31 * result + (assetPath?.hashCode() ?: 0)
        result = 31 * result + (resourceId ?: 0)
        result = 31 * result + ttcIndex
        result = 31 * result + (weight ?: 0)
        result = 31 * result + italic.hashCode()
        result = 31 * result + axes.hashCode()
        result = 31 * result + languageTags.hashCode()
        result = 31 * result + (fallbackFor?.hashCode() ?: 0)
        result = 31 * result + (sourceId?.hashCode() ?: 0)
        return result
    }
}

private fun bytesEqual(left: ByteArray?, right: ByteArray?): Boolean {
    return when {
        left === right -> true
        left == null || right == null -> false
        else -> left.contentEquals(right)
    }
}

data class NativeFontAxis(
    val tag: String,
    val value: Float
) {
    init {
        require(tag.length == 4) {
            "OpenType axis tag must be 4 characters"
        }
    }
}

data class NativeFontConfig(
    val primary: NativeFontSource?,
    val fallbacks: List<NativeFontSource>
) {
    fun stableKey(): String {
        return buildString {
            append(primary?.stableKey() ?: "no-primary")
            append("::")
            append(fallbacks.joinToString(";;") { it.stableKey() })
        }
    }
}

fun List<NativeFontSource>.prioritizeForNativeLyricsLocale(locale: String): List<NativeFontSource> {
    val normalizedLocale = locale.lowercase()
    if (!normalizedLocale.startsWith("zh") &&
        !normalizedLocale.startsWith("ja") &&
        !normalizedLocale.startsWith("ko")
    ) {
        return this
    }

    return withIndex()
        .sortedWith(
            compareBy<IndexedValue<NativeFontSource>> { it.value.cjkLocalePriority(normalizedLocale) }
                .thenBy { it.index }
        )
        .map { it.value }
}

private fun NativeFontSource.cjkLocalePriority(locale: String): Int {
    notoCjkTtcLocalePriority(locale)?.let { return it }

    val tags = languageTags.map { it.lowercase() }
    return when {
        locale.startsWith("ja") -> tags.priorityOf(
            "ja" to 0,
            "zh-hans" to 1,
            "zh-cn" to 1,
            "zh" to 2,
            "zh-hant" to 3,
            "zh-tw" to 3,
            "zh-hk" to 3,
            "zh-mo" to 3,
            "ko" to 4
        )
        locale.startsWith("ko") -> tags.priorityOf(
            "ko" to 0,
            "zh-hans" to 1,
            "zh-cn" to 1,
            "zh" to 2,
            "zh-hant" to 3,
            "zh-tw" to 3,
            "zh-hk" to 3,
            "zh-mo" to 3,
            "ja" to 4
        )
        locale.startsWith("zh-hant") ||
            locale.startsWith("zh-tw") ||
            locale.startsWith("zh-hk") ||
            locale.startsWith("zh-mo") -> tags.priorityOf(
                "zh-hant" to 0,
                "zh-tw" to 0,
                "zh-hk" to 0,
                "zh-mo" to 0,
                "zh" to 1,
                "zh-hans" to 2,
                "zh-cn" to 2,
                "ja" to 3,
                "ko" to 4
            )
        else -> tags.priorityOf(
            "zh-hans" to 0,
            "zh-cn" to 0,
            "zh" to 1,
            "zh-hant" to 2,
            "zh-tw" to 2,
            "zh-hk" to 2,
            "zh-mo" to 2,
            "ja" to 3,
            "ko" to 4
        )
    }
}

private fun NativeFontSource.notoCjkTtcLocalePriority(locale: String): Int? {
    val id = listOfNotNull(path, assetPath, sourceId)
        .joinToString("|")
        .lowercase()
    if (!id.contains("notosanscjk") && !id.contains("notoserifcjk")) {
        return null
    }

    val targetIndex = when {
        locale.startsWith("ja") -> 0
        locale.startsWith("ko") -> 1
        locale.startsWith("zh-hant") ||
            locale.startsWith("zh-tw") ||
            locale.startsWith("zh-hk") ||
            locale.startsWith("zh-mo") -> 3
        locale.startsWith("zh") -> 2
        else -> return null
    }

    return when (ttcIndex) {
        targetIndex -> -20
        2 -> if (targetIndex == 3) 20 else 30
        3 -> if (targetIndex == 2) 20 else 30
        0 -> if (targetIndex == 0) -20 else 40
        1 -> if (targetIndex == 1) -20 else 40
        4 -> if (targetIndex == 3) 21 else 41
        else -> 50
    }
}

private fun List<String>.priorityOf(vararg priorities: Pair<String, Int>): Int {
    for ((prefix, priority) in priorities) {
        if (any { tag -> tag == prefix || tag.startsWith("$prefix-") }) {
            return priority
        }
    }
    return 5
}

/**
 * Native text engine for SDF glyph generation and text layout.
 *
 * The atlas dimensions are fixed for the lifetime of the engine. Font state is
 * configured atomically through [configureFonts], which clears native glyph
 * caches and pending uploads.
 */
expect class NativeTextEngine(
    atlasWidth: Int = 2048,
    atlasHeight: Int = 2048
) {
    val atlasWidth: Int
    val atlasHeight: Int
    internal val generation: Int

    fun configureFonts(config: NativeFontConfig): Boolean

    fun processText(text: String, sizePx: Float, weight: Float = 400f): String

    fun hasPendingUploads(): Boolean

    internal fun getPendingUploadsJson(): String

    fun getAtlasSize(): String

    fun setLyricsScene(sceneJson: String): String

    fun getLyricsRendererMetrics(): String

    fun close()
}

fun NativeTextEngine.drainPendingUploads(): List<GlyphUpload> {
    if (!hasPendingUploads()) return emptyList()
    return parsePendingUploads(getPendingUploadsJson())
}
