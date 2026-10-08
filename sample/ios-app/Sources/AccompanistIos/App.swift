import SwiftUI
import UIKit
import AVFoundation
import UniformTypeIdentifiers
import AccompanistSample

@main
struct AccompanistApp: App {
    var body: some Scene { WindowGroup { PlayerView() } }
}

private struct LyricsView: UIViewControllerRepresentable {
    let controller: IosLyricsController
    func makeUIViewController(context: Context) -> UIViewController {
        MainViewControllerKt.MainViewController(controller: controller)
    }
    func updateUIViewController(_ controller: UIViewController, context: Context) {}
}

@MainActor
private final class FrameTarget: NSObject {
    weak var player: PlayerModel?
    @objc func frame(_ link: CADisplayLink) { player?.frame() }
}

@MainActor
private final class PlayerModel: NSObject, ObservableObject {
    let controller = IosLyricsController()
    private var duration = 1.0
    private var playing = false
    @Published var importRequest: Int32 = 0
    @Published var error: String?
    private var audio: AVAudioPlayer?
    private var link: CADisplayLink?
    private let target = FrameTarget()
    private var previewPosition = 0.0
    private var previewStarted = CACurrentMediaTime()
    private var metadataTask: Task<Void, Never>?
    private var selectedFiles: [Int32: URL] = [:]
    private var pendingAudio: (player: AVAudioPlayer, url: URL)?
    private var activeAudioURL: URL?
    private let stagingDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("AccompanistSelection", isDirectory: true)

    override init() {
        super.init()
        target.player = self
        controller.openLyrics(content: "[00:00.00]Accompanist on iOS\n[00:04.00]Open lyrics and audio files\n[00:08.00]Tap a line to seek\n[00:12.00]Switch translation and pronunciation\n[00:16.00]", name: "Preview")
    }

    func startFrames() {
        guard link == nil else { return }
        link = CADisplayLink(target: target, selector: #selector(FrameTarget.frame(_:)))
        link?.add(to: .main, forMode: .common)
    }
    func stopFrames() { link?.invalidate(); link = nil }
    private var current: Double {
        audio?.currentTime ?? (previewPosition + (playing ? CACurrentMediaTime() - previewStarted : 0))
    }
    func frame() {
        if controller.takeSelectionCancelRequest() { pendingAudio = nil }
        if controller.takeSelectionPlayRequest() { prepareSelection() }
        if controller.takePreparedSelection() { playSelection() }
        if controller.takePlaybackToggle() { toggle() }
        let file = controller.takeFileRequest()
        if file != 0 { importRequest = file }
        let requested = controller.takeSeekRequest()
        if requested >= 0 { seek(Double(requested) / 1000) }
        let length = audio?.duration ?? Double(controller.durationMillis) / 1000
        if length > 0 && duration != length { duration = length }
        let now = min(current, duration)
        controller.updatePosition(millis: Int32(clamping: Int(now * 1000)))
        if playing && (now >= duration || (audio != nil && audio?.isPlaying == false)) {
            previewPosition = now
            playing = false
        }
        controller.updatePlaybackStatus(isPlaying: playing,
            duration: Int32(clamping: Int(duration * 1000)))
    }
    func toggle() {
        if playing {
            previewPosition = current
            audio?.pause()
            playing = false
        } else {
            if current >= duration { seek(0) }
            previewStarted = CACurrentMediaTime()
            if let audio {
                do {
                    try AVAudioSession.sharedInstance().setCategory(.playback)
                    try AVAudioSession.sharedInstance().setActive(true)
                    guard audio.play() else { throw CocoaError(.fileReadUnknown) }
                } catch { self.error = error.localizedDescription; return }
            }
            playing = true
        }
    }
    func seek(_ seconds: Double) {
        let value = min(max(seconds, 0), duration)
        audio?.currentTime = value
        previewPosition = value
        previewStarted = CACurrentMediaTime()
        controller.updatePosition(millis: Int32(clamping: Int(value * 1000)))
    }
    func select(_ url: URL, kind: Int32) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            // Keep picker files in our sandbox while the shared dialog stays open.
            let folder = stagingDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let local = folder.appendingPathComponent(url.lastPathComponent)
            do { try FileManager.default.copyItem(at: url, to: local) }
            catch { try? FileManager.default.removeItem(at: folder); throw error }
            if let previous = selectedFiles[kind], previous != activeAudioURL {
                try? FileManager.default.removeItem(at: previous.deletingLastPathComponent())
            }
            selectedFiles[kind] = local
            if kind == 2 {
                for optional: Int32 in [1, 3] {
                    if let previous = selectedFiles.removeValue(forKey: optional) {
                        try? FileManager.default.removeItem(at: previous.deletingLastPathComponent())
                    }
                }
            }
            controller.setSelectedFile(kind: kind, name: url.lastPathComponent)
        } catch { controller.reportSelectionError(error: error.localizedDescription) }
    }

    private func readLyrics(_ url: URL?) throws -> String? {
        guard let url else { return nil }
        let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
        guard size <= 16 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
        let data = try Data(contentsOf: url)
        guard let content = String(data: data, encoding: .utf8) ?? String(data: data, encoding: .utf16) else {
            throw CocoaError(.fileReadInapplicableStringEncoding)
        }
        return content
    }

    private func prepareSelection() {
        do {
            guard let url = selectedFiles[2] else { throw CocoaError(.fileNoSuchFile) }
            let player = try AVAudioPlayer(contentsOf: url)
            guard player.prepareToPlay() else { throw CocoaError(.fileReadUnknown) }
            let lyrics = try readLyrics(selectedFiles[1])
            let translation = try readLyrics(selectedFiles[3])
            pendingAudio = (player, url)
            controller.prepareSelectedLyrics(content: lyrics,
                name: selectedFiles[1]?.lastPathComponent ?? url.lastPathComponent,
                translation: translation)
        } catch {
            pendingAudio = nil
            controller.reportSelectionError(error: error.localizedDescription)
        }
    }

    private func playSelection() {
        guard let pending = pendingAudio else { return }
        pendingAudio = nil
        do {
            try AVAudioSession.sharedInstance().setCategory(.playback)
            try AVAudioSession.sharedInstance().setActive(true)
            guard pending.player.play() else { throw CocoaError(.fileReadUnknown) }
            let previousAudioURL = activeAudioURL
            audio?.stop()
            audio = pending.player
            activeAudioURL = pending.url
            duration = pending.player.duration
            playing = true
            previewPosition = 0
            previewStarted = CACurrentMediaTime()
            controller.completeSelection()
            loadMetadata(pending.url)
            if let previousAudioURL, !selectedFiles.values.contains(previousAudioURL) {
                try? FileManager.default.removeItem(at: previousAudioURL.deletingLastPathComponent())
            }
        } catch { controller.reportSelectionError(error: error.localizedDescription) }
    }

    private func loadMetadata(_ url: URL) {
        metadataTask?.cancel()
        let fallback = url.deletingPathExtension().lastPathComponent
        controller.setTrackMetadata(trackTitle: fallback, trackArtist: "Unknown", artwork: nil)
        metadataTask = Task {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            do {
                let items = try await AVURLAsset(url: url).load(.commonMetadata)
                var title = fallback
                var artist = "Unknown"
                var artwork: Data?
                for item in items {
                    switch item.commonKey {
                    case .commonKeyTitle:
                        title = try await item.load(.stringValue) ?? fallback
                    case .commonKeyArtist:
                        artist = try await item.load(.stringValue) ?? "Unknown"
                    case .commonKeyArtwork:
                        artwork = try await item.load(.dataValue)
                    default: break
                    }
                }
                guard !Task.isCancelled else { return }
                controller.setTrackMetadata(trackTitle: title, trackArtist: artist,
                    artwork: artwork)
            } catch {
                // Audio without readable tags still keeps its filename and default background.
            }
        }
    }
}

private struct PlayerView: View {
    private enum ImportKind {
        case lyrics, audio, translation

        var contentTypes: [UTType] { self == .audio ? [.audio] : [.data] }
    }

    @StateObject private var player = PlayerModel()
    @State private var importKind = ImportKind.lyrics
    @State private var chooseFile = false

    var body: some View {
        LyricsView(controller: player.controller)
        .ignoresSafeArea().preferredColorScheme(.dark)
        .onAppear { player.startFrames() }.onDisappear { player.stopFrames() }
        .onChange(of: player.importRequest) { request in
            guard request != 0 else { return }
            importKind = request == 2 ? .audio : (request == 3 ? .translation : .lyrics)
            chooseFile = true
            player.importRequest = 0
        }
        // Audio, lyrics and translation share one system document presenter.
        .fileImporter(isPresented: $chooseFile, allowedContentTypes: importKind.contentTypes) { result in
            switch result {
            case .success(let url):
                player.select(url, kind: importKind == .audio ? 2 : (importKind == .translation ? 3 : 1))
            case .failure(let error):
                // Cancelling the system picker leaves the selection dialog intact.
                if (error as NSError).code != CocoaError.userCancelled.rawValue {
                    player.controller.reportSelectionError(error: error.localizedDescription)
                }
            }
        }
        .alert("Unable to open file", isPresented: Binding(get: { player.error != nil }, set: { if !$0 { player.error = nil } })) {
            Button("OK") { player.error = nil }
        } message: { Text(player.error ?? "") }
    }
}
