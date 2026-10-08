package com.mocharealm.accompanist.sample.desktop

import kotlinx.serialization.json.*
import java.nio.file.Files
import java.nio.file.Path
import kotlin.io.path.*

internal data class DesktopConfig(
    val configFile: Path,
    val lyricsDir: Path,
    val recursive: Boolean = false,
    val gpu: String = "system",
    val frameTiming: Boolean = false,
    val width: Int = 420,
    val height: Int = 520,
    val minWidth: Int = 320,
    val minHeight: Int = 240,
    val alwaysOnTop: Boolean = true,
) {
    companion object {
        fun load(args: Array<String>): DesktopConfig {
            val options = mutableMapOf<String, String>()
            var index = 0
            while (index < args.size) {
                val key = args[index++]
                when (key) {
                    "--recursive", "--frame-timing" -> options[key] = "true"
                    "--config", "--lyrics-dir", "--gpu" -> {
                        require(index < args.size) { "$key requires a value" }
                        options[key] = args[index++]
                    }
                    "--help", "-h" -> {
                        println("Accompanist [--config file] [--lyrics-dir folder] [--recursive] " +
                            "[--gpu system|high_performance|minimum_power] [--frame-timing]")
                        kotlin.system.exitProcess(0)
                    }
                    else -> error("Unknown argument: $key")
                }
            }
            val configFile = options["--config"]?.let { Path.of(it).toAbsolutePath() }
                ?: defaultDirectory().resolve("config.json")
            if (options.containsKey("--config")) require(configFile.isRegularFile()) {
                "Config file does not exist: $configFile"
            }
            val document = if (configFile.exists())
                Json.parseToJsonElement(configFile.readText()).jsonObject else JsonObject(emptyMap())
            val window = document["window"]?.jsonObject ?: JsonObject(emptyMap())
            fun text(key: String, fallback: String) = document[key]?.jsonPrimitive?.content ?: fallback
            fun flag(option: String, environment: String, key: String, fallback: Boolean): Boolean =
                (options[option] ?: System.getenv(environment))?.let(::enabled)
                    ?: document[key]?.jsonPrimitive?.booleanOrNull ?: fallback
            val directory = options["--lyrics-dir"] ?: System.getenv("ACCOMPANIST_LYRICS_DIR")
                ?: text("lyrics_dir", "lyrics")
            val directoryPath = Path.of(directory)
            val lyricsDir = if (directoryPath.isAbsolute) directoryPath else {
                val base = if (options.containsKey("--lyrics-dir") ||
                    System.getenv("ACCOMPANIST_LYRICS_DIR") != null) Path.of("").toAbsolutePath()
                else configFile.parent
                base.resolve(directoryPath).normalize()
            }
            val gpu = normalizeGpu(options["--gpu"] ?: System.getenv("ACCOMPANIST_GPU")
                ?: text("gpu", "system"))
            fun dimension(key: String, fallback: Int) =
                (window[key]?.jsonPrimitive?.intOrNull ?: fallback).coerceIn(100, 8192)
            val config = DesktopConfig(configFile, lyricsDir,
                flag("--recursive", "ACCOMPANIST_LYRICS_RECURSIVE", "recursive", false), gpu,
                flag("--frame-timing", "ACCOMPANIST_FRAME_TIMING", "frame_timing", false),
                dimension("width", 420), dimension("height", 520),
                dimension("min_width", 320), dimension("min_height", 240),
                window["always_on_top"]?.jsonPrimitive?.booleanOrNull ?: true)
            Files.createDirectories(lyricsDir)
            if (!configFile.exists()) {
                Files.createDirectories(configFile.parent)
                configFile.writeText(buildJsonObject {
                    put("lyrics_dir", lyricsDir.toString()); put("recursive", config.recursive)
                    put("gpu", config.gpu); put("frame_timing", config.frameTiming)
                    putJsonObject("window") {
                        put("width", config.width); put("height", config.height)
                        put("min_width", config.minWidth); put("min_height", config.minHeight)
                        put("always_on_top", config.alwaysOnTop)
                    }
                }.toString())
            }
            return config
        }

        private fun defaultDirectory(): Path {
            val home = Path.of(System.getProperty("user.home"))
            return when (desktopPlatform()) {
                DesktopPlatform.Windows -> Path.of(System.getenv("LOCALAPPDATA") ?: home.toString())
                    .resolve("Accompanist")
                DesktopPlatform.Mac -> home.resolve("Library/Application Support/Accompanist")
                DesktopPlatform.Linux -> Path.of(System.getenv("XDG_CONFIG_HOME")
                    ?: home.resolve(".config").toString()).resolve("accompanist-lyrics")
            }
        }
        private fun enabled(value: String) = value.lowercase() in listOf("1", "true", "yes")
        private fun normalizeGpu(value: String) = when (value.lowercase()) {
            "auto", "system" -> "system"
            "high", "performance", "high_performance", "dgpu", "discrete" -> "high_performance"
            "low", "power", "saving", "minimum_power", "igpu", "integrated" -> "minimum_power"
            else -> error("Unknown GPU preference: $value")
        }
    }
}

internal enum class DesktopPlatform { Windows, Linux, Mac }
internal fun desktopPlatform(): DesktopPlatform {
    val os = System.getProperty("os.name").lowercase()
    return when {
        os.contains("mac") -> DesktopPlatform.Mac
        os.contains("win") -> DesktopPlatform.Windows
        else -> DesktopPlatform.Linux
    }
}

internal fun configureDesktopGraphics(config: DesktopConfig) {
    System.setProperty("skiko.gpu.priority", when (config.gpu) {
        "high_performance" -> "discrete"
        "minimum_power" -> "integrated"
        else -> "auto"
    })
    if (config.frameTiming) {
        System.setProperty("accompanist.frameTiming", "true")
        System.setProperty("skiko.fps.enabled", "true")
        System.setProperty("skiko.fps.longFrames.show", "true")
    }
}
