import AVFoundation
import Foundation

/// Runtime audio for state-machine sounds (issue #111).
///
/// A `.kvk` carries no audio bytes — it names the sounds a machine can fire,
/// and the app ships the files. So playback here is two halves: a
/// ``KinetoAudioLibrary`` that says which file a named sound means, and a
/// ``KinetoAudioDirector`` that asks the player what fired and plays it.
///
/// The engine reports; nothing in Rust makes a noise.

// MARK: - The sound seam

/// One playing sound.
///
/// A protocol rather than `AVAudioPlayer` directly, for the same reason the
/// editor's `AudioVoice` is one: the whole director can then be exercised in a
/// test with no audio device, which is where every rule below is actually
/// pinned.
public protocol KinetoSound: AnyObject {
    func play()
    func stop()
    /// Linear gain, 0...1. Timeline clips carry an authored volume; one-shots
    /// do not, so this has a default that ignores it.
    func setVolume(_ volume: Double)
    /// Moves playback to `position` seconds into the file. Only the timeline
    /// needs this — a one-shot always starts at the beginning.
    func seek(to position: TimeInterval)
    /// Where playback has got to, when the implementation can say. `nil`
    /// means "cannot tell", and the timeline then leaves it alone rather than
    /// seeking blind.
    var currentPosition: TimeInterval? { get }
}

public extension KinetoSound {
    func setVolume(_ volume: Double) {}
    func seek(to position: TimeInterval) {}
    var currentPosition: TimeInterval? { nil }
}

/// Makes sounds from files.
public protocol KinetoSoundFactory {
    /// A sound for `url`, or nil when the file cannot be opened. A file that
    /// will not open is not an error the machine should notice.
    func makeSound(for url: URL) -> KinetoSound?
}

extension AVAudioPlayer: KinetoSound {
    public func play() {
        _ = (self as AVAudioPlayer).play() as Bool
    }

    public func setVolume(_ volume: Double) {
        self.volume = Float(volume)
    }

    public func seek(to position: TimeInterval) {
        currentTime = position
    }

    public var currentPosition: TimeInterval? {
        currentTime
    }
}

/// The real factory: one `AVAudioPlayer` per firing.
public struct KinetoAVAudioSoundFactory: KinetoSoundFactory {
    public init() {}

    public func makeSound(for url: URL) -> KinetoSound? {
        guard let player = try? AVAudioPlayer(contentsOf: url) else {
            return nil
        }
        player.prepareToPlay()
        return player
    }
}

// MARK: - Library

/// Which file each named sound means.
///
/// The `.kvk` gives three handles onto a sound — its id, the name the author
/// gave it, and the file it was authored against — and an app may recognise
/// any of them. Resolution tries them in that order: an id is exact, a name is
/// what a designer says out loud, and the source file name is the last resort
/// for an app that simply ships the same files.
public struct KinetoAudioLibrary {
    private var byEventID: [String: URL] = [:]
    private var byName: [String: URL] = [:]
    private var bySourceName: [String: URL] = [:]

    public init() {}

    /// Binds a file to an event id — the exact form, and the one to prefer.
    public mutating func bind(eventID: String, to url: URL) {
        byEventID[eventID] = url
    }

    /// Binds a file to the name the author gave the sound (`ClickSound`).
    public mutating func bind(name: String, to url: URL) {
        byName[name] = url
    }

    /// Binds a file to the name of the file the sound was authored against
    /// (`click.wav`).
    public mutating func bind(sourceName: String, to url: URL) {
        bySourceName[sourceName] = url
    }

    /// Binds every event whose `sourceName` names a file `bundle` carries,
    /// which is the case when the app ships the same files the document was
    /// authored with.
    ///
    /// Returns the events it could not place, so a host can log them rather
    /// than discover the silence later.
    @discardableResult
    public mutating func bindMatchingFiles(
        for events: [KinetoPlayer.AudioEvent],
        in bundle: Bundle
    ) -> [KinetoPlayer.AudioEvent] {
        var unresolved: [KinetoPlayer.AudioEvent] = []
        for event in events {
            guard let source = event.sourceName else {
                unresolved.append(event)
                continue
            }
            let name = (source as NSString).deletingPathExtension
            let ext = (source as NSString).pathExtension
            guard
                !name.isEmpty,
                !ext.isEmpty,
                let url = bundle.url(forResource: name, withExtension: ext)
            else {
                unresolved.append(event)
                continue
            }
            bind(eventID: event.id, to: url)
        }
        return unresolved
    }

    /// The file `event` should play, or nil when the app ships none.
    public func url(for event: KinetoPlayer.AudioEvent) -> URL? {
        if let url = byEventID[event.id] {
            return url
        }
        if let url = byName[event.name] {
            return url
        }
        if let source = event.sourceName, let url = bySourceName[source] {
            return url
        }
        return nil
    }
}

// MARK: - Director

/// Plays the sounds a machine fires.
///
/// Drive it from the same place that drives the machine: call ``pump()`` after
/// `advance` and after every pointer event, because a listener fires outside
/// any tick. ``KinetoPlayerViewController`` does this for you when you hand it
/// one.
///
/// ## Rules it keeps
///
/// * One firing is one sound. Nothing is coalesced: the same sound twice is
///   two overlapping players, which is what a double-tap is.
/// * A sound the app ships no file for is silent and *counted* — never an
///   error, because a machine must keep running.
/// * ``stopAll()`` silences everything this director started and nothing else.
public final class KinetoAudioDirector {
    /// Why a firing made no sound.
    public enum Miss: Equatable {
        /// The event id is not in the document — a stale reference.
        case unknownEvent(String)
        /// The document names the sound; the app ships no file for it.
        case noFile(KinetoPlayer.AudioEvent)
        /// The file is there but would not open.
        case unplayable(KinetoPlayer.AudioEvent, URL)
    }

    public var library: KinetoAudioLibrary

    /// Called for every firing that made no sound. Diagnostics, not an error
    /// path: nothing here stops the machine.
    public var onMiss: ((Miss) -> Void)?

    /// Called for every firing that did make a sound.
    ///
    /// A host that logs firings needs both halves: without this, a log shows
    /// only failures, and "nothing happened" is indistinguishable from
    /// "everything worked".
    public var onPlay: ((KinetoPlayer.AudioEvent) -> Void)?

    private let factory: KinetoSoundFactory
    private var catalogue: [String: KinetoPlayer.AudioEvent]
    private var playing: [KinetoSound] = []

    /// - Parameters:
    ///   - events: the document's sounds, from `KinetoPlayer.audioEvents()`.
    ///   - library: which file each one means.
    ///   - factory: how a file becomes a sound. Substituted in tests.
    public init(
        events: [KinetoPlayer.AudioEvent],
        library: KinetoAudioLibrary = KinetoAudioLibrary(),
        factory: KinetoSoundFactory = KinetoAVAudioSoundFactory()
    ) {
        self.catalogue = Dictionary(uniqueKeysWithValues: events.map { ($0.id, $0) })
        self.library = library
        self.factory = factory
    }

    /// Builds a director for `player`, reading its catalogue.
    public convenience init(
        player: KinetoPlayer,
        library: KinetoAudioLibrary = KinetoAudioLibrary(),
        factory: KinetoSoundFactory = KinetoAVAudioSoundFactory()
    ) {
        self.init(events: player.audioEvents(), library: library, factory: factory)
    }

    /// How many sounds this director currently owns. Test and diagnostic seam.
    public var activeSoundCount: Int {
        playing.count
    }

    /// Plays whatever `player` has fired since the last call.
    ///
    /// Returns the number of sounds actually started, so a caller can log the
    /// difference between "nothing fired" and "everything was missing".
    @discardableResult
    public func pump(_ player: KinetoPlayer) -> Int {
        play(player.takeFiredAudioEvents())
    }

    /// Plays `eventIDs` in order — the shape the runtime reports firings in.
    @discardableResult
    public func play(_ eventIDs: [String]) -> Int {
        var played = 0
        for id in eventIDs {
            guard let event = catalogue[id] else {
                onMiss?(.unknownEvent(id))
                continue
            }
            guard let url = library.url(for: event) else {
                onMiss?(.noFile(event))
                continue
            }
            guard let sound = factory.makeSound(for: url) else {
                onMiss?(.unplayable(event, url))
                continue
            }
            playing.append(sound)
            sound.play()
            onPlay?(event)
            played += 1
        }
        return played
    }

    /// Stops every sound this director started.
    ///
    /// What a host calls when the machine stops, the document is swapped, or
    /// the view goes away.
    public func stopAll() {
        let sounds = playing
        playing.removeAll()
        for sound in sounds {
            sound.stop()
        }
    }

    /// Drops finished sounds, so a long session does not accumulate players.
    ///
    /// Only `AVAudioPlayer` can answer "are you still going"; a substituted
    /// factory's sounds are left to ``stopAll()``.
    public func collectFinished() {
        playing.removeAll { sound in
            guard let player = sound as? AVAudioPlayer else {
                return false
            }
            return !player.isPlaying
        }
    }
}

// MARK: - Audio session

public enum KinetoAudioSession {
    /// Configures the process audio session so runtime sounds are audible.
    ///
    /// `.playback` on purpose: a state-machine sound is part of what the app
    /// is showing, not an alert, so it should not be silenced by the ring
    /// switch. An app that wants the opposite — sounds that respect the
    /// switch — should skip this and set `.ambient` itself.
    ///
    /// Returns the error when the session refuses, rather than throwing: audio
    /// that cannot be configured is a quiet app, not a broken one.
    @discardableResult
    public static func activatePlayback() -> Error? {
        #if os(iOS)
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playback, mode: .default)
            try session.setActive(true)
            return nil
        } catch {
            return error
        }
        #else
        return nil
        #endif
    }
}

// MARK: - Timeline audio

/// Keeps the timeline's soundtrack in step with the playhead (issue #111).
///
/// The state-machine director plays one-shots: something fired, play it once,
/// forget it. A timeline clip is the opposite — it is *continuous*, it has a
/// position, and the runtime says every frame what should be sounding and
/// where. So this converges rather than triggers: start what appeared, stop
/// what disappeared, and re-seek what has drifted.
///
/// That is the same contract the editor's `AudioPlaybackCoordinator` keeps,
/// deliberately, so preview and runtime cannot disagree about what a document
/// sounds like.
public final class KinetoTimelineAudio {
    /// How far a player may drift from the runtime's position before it is
    /// seeked back. Below this, seeking does more damage than the drift.
    ///
    /// The runtime's clock and the audio device's clock are independent — one
    /// advances by the display link's delta, the other by the hardware — so
    /// they diverge slowly no matter what. Re-seeking on that divergence
    /// clicks, periodically and forever. The tolerance is therefore set to
    /// catch a *jump* (a loop, a scrub) rather than drift; a quarter second of
    /// slip is inaudible against a soundtrack, and a click is not.
    public var driftTolerance: TimeInterval = 0.75

    /// Whether the timeline is allowed to make sound at all.
    ///
    /// A host that stops playback needs the silence to stick. `stopAll()` on
    /// its own does not: the next `sync` sees the same clip still audible and
    /// starts it again, so "stop" lasted until the next frame — which is
    /// exactly how it behaved in the reference app (issue #111).
    public var isEnabled = true

    public var library: KinetoAudioLibrary

    /// Called when a clip that should be sounding has no file bound.
    public var onMissingFile: ((KinetoPlayer.ActiveAudioClip) -> Void)?

    /// Called when a clip starts sounding, and when one stops.
    ///
    /// Timeline audio is continuous, so "did anything happen" is otherwise
    /// invisible: without these a silent timeline and a working one produce
    /// exactly the same empty log.
    public var onStart: ((KinetoPlayer.ActiveAudioClip) -> Void)?
    public var onStop: ((String) -> Void)?

    private let factory: KinetoSoundFactory
    private var voices: [String: KinetoSound] = [:]

    public init(
        library: KinetoAudioLibrary = KinetoAudioLibrary(),
        factory: KinetoSoundFactory = KinetoAVAudioSoundFactory()
    ) {
        self.library = library
        self.factory = factory
    }

    /// Track ids currently sounding. Test and diagnostic seam.
    public var activeTrackIDs: Set<String> {
        Set(voices.keys)
    }

    /// Brings playback in line with `clips`, the runtime's answer for this
    /// frame.
    public func sync(_ clips: [KinetoPlayer.ActiveAudioClip]) {
        guard isEnabled else {
            // Not `stopAll()`: that would clear the flag it was called for.
            silence()
            return
        }
        let wanted = Dictionary(uniqueKeysWithValues: clips.map { ($0.trackID, $0) })

        // Anything no longer audible stops — a clip that ended, a playhead
        // that jumped away, a document that changed.
        for (trackID, voice) in voices where wanted[trackID] == nil {
            voice.stop()
            voices.removeValue(forKey: trackID)
            onStop?(trackID)
        }

        for clip in clips {
            if let voice = voices[clip.trackID] {
                voice.setVolume(clip.volume)
                // A playhead jump shows up as drift; ordinary playback does
                // not, because the player advances on its own.
                if let position = voice.currentPosition,
                   abs(position - clip.sourcePosition) > driftTolerance {
                    // Stop before seeking, then start again. Moving the
                    // playhead of a *sounding* AVAudioPlayer clicks; the jump
                    // that got us here is usually a loop, where a clean
                    // restart is what the ear expects anyway.
                    voice.stop()
                    voice.seek(to: clip.sourcePosition)
                    voice.play()
                }
                continue
            }

            guard let url = library.url(forFileNamed: clip.fileName, assetID: clip.assetID)
            else {
                onMissingFile?(clip)
                continue
            }
            guard let voice = factory.makeSound(for: url) else {
                onMissingFile?(clip)
                continue
            }
            voice.setVolume(clip.volume)
            voice.seek(to: clip.sourcePosition)
            voice.play()
            voices[clip.trackID] = voice
            onStart?(clip)
        }
    }

    /// Stops every clip and keeps it stopped until [isEnabled] is set again.
    ///
    /// What a host calls for a stop button. Silence that a later frame can
    /// undo is not a stop.
    public func stopAll() {
        isEnabled = false
        silence()
    }

    /// Stops every clip without touching [isEnabled].
    ///
    /// What a host calls on pause or when the view goes away: a timeline that
    /// is not moving makes no sound, and resuming should just work.
    public func pauseAll() {
        silence()
    }

    private func silence() {
        for voice in voices.values {
            voice.stop()
        }
        voices.removeAll()
    }
}

public extension KinetoAudioLibrary {
    /// Whether this library can supply the file `fileName`, however it was
    /// bound.
    ///
    /// What a host asks to show "have I added this yet?" — the question every
    /// document raises, since a `.kvk` names its audio and carries none.
    func hasBinding(forFileNamed fileName: String, assetID: String) -> Bool {
        url(forFileNamed: fileName, assetID: assetID) != nil
    }

    /// Resolves a timeline clip's file: by asset id when the host bound one,
    /// otherwise by the file name the document recorded.
    func url(forFileNamed fileName: String, assetID: String) -> URL? {
        url(
            for: KinetoPlayer.AudioEvent(
                id: assetID,
                name: fileName,
                sourceName: fileName
            )
        )
    }
}
