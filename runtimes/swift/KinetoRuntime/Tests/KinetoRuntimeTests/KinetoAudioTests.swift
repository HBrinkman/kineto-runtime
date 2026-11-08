import XCTest
@testable import KinetoRuntime

/// Issue #111 — the runtime plays a machine's sounds, and every rule about
/// *when* is pinned here with a fake sound factory, so no audio device is
/// involved.
final class KinetoAudioTests: XCTestCase {
    private let click = KinetoPlayer.AudioEvent(
        id: "event-click",
        name: "ClickSound",
        sourceName: "click.wav"
    )
    private let pop = KinetoPlayer.AudioEvent(
        id: "event-pop",
        name: "PopSound",
        sourceName: nil
    )

    private func url(_ name: String) -> URL {
        URL(fileURLWithPath: "/tmp/\(name)")
    }

    // MARK: - Library

    func testResolvesByEventIDFirst() {
        var library = KinetoAudioLibrary()
        library.bind(name: "ClickSound", to: url("by-name.wav"))
        library.bind(eventID: "event-click", to: url("by-id.wav"))

        // The id is exact; a name is a convenience. When both are bound the
        // exact one has to win.
        XCTAssertEqual(library.url(for: click), url("by-id.wav"))
    }

    func testFallsBackToNameThenSourceName() {
        var byName = KinetoAudioLibrary()
        byName.bind(name: "ClickSound", to: url("named.wav"))
        XCTAssertEqual(byName.url(for: click), url("named.wav"))

        var bySource = KinetoAudioLibrary()
        bySource.bind(sourceName: "click.wav", to: url("source.wav"))
        XCTAssertEqual(bySource.url(for: click), url("source.wav"))
    }

    func testUnboundSoundResolvesToNothing() {
        XCTAssertNil(KinetoAudioLibrary().url(for: click))
    }

    // MARK: - Director

    func testOneFiringIsOneSound() {
        let factory = FakeSoundFactory()
        let director = makeDirector(factory: factory, bind: [click])

        XCTAssertEqual(director.play(["event-click"]), 1)
        XCTAssertEqual(factory.made.count, 1)
        XCTAssertTrue(factory.made[0].isPlaying)
        XCTAssertEqual(director.activeSoundCount, 1)
    }

    func testTheSameSoundTwiceIsTwoOverlappingSounds() {
        let factory = FakeSoundFactory()
        let director = makeDirector(factory: factory, bind: [click])

        // Never coalesced: that is what a double-tap sounds like.
        XCTAssertEqual(director.play(["event-click", "event-click"]), 2)
        XCTAssertEqual(factory.made.count, 2)
        XCTAssertEqual(director.activeSoundCount, 2)
    }

    func testSoundsPlayInTheOrderTheyFired() {
        let factory = FakeSoundFactory()
        var library = KinetoAudioLibrary()
        library.bind(eventID: "event-click", to: url("click.wav"))
        library.bind(eventID: "event-pop", to: url("pop.wav"))
        let director = KinetoAudioDirector(
            events: [click, pop],
            library: library,
            factory: factory
        )

        director.play(["event-pop", "event-click"])

        XCTAssertEqual(
            factory.made.map(\.url.lastPathComponent),
            ["pop.wav", "click.wav"]
        )
    }

    func testASoundWithNoFileIsSilentAndReported() {
        let factory = FakeSoundFactory()
        var misses: [KinetoAudioDirector.Miss] = []
        let director = KinetoAudioDirector(
            events: [click],
            library: KinetoAudioLibrary(),
            factory: factory
        )
        director.onMiss = { misses.append($0) }

        XCTAssertEqual(director.play(["event-click"]), 0)
        XCTAssertTrue(factory.made.isEmpty)
        XCTAssertEqual(misses, [.noFile(click)])
    }

    func testAnIDNamingNoSoundIsReportedAsUnknown() {
        var misses: [KinetoAudioDirector.Miss] = []
        let director = makeDirector(factory: FakeSoundFactory(), bind: [click])
        director.onMiss = { misses.append($0) }

        XCTAssertEqual(director.play(["event-gone"]), 0)
        XCTAssertEqual(misses, [.unknownEvent("event-gone")])
    }

    func testAFileThatWillNotOpenIsReportedAndDoesNotThrow() {
        let factory = FakeSoundFactory()
        factory.refuses = true
        var misses: [KinetoAudioDirector.Miss] = []
        let director = makeDirector(factory: factory, bind: [click])
        director.onMiss = { misses.append($0) }

        XCTAssertEqual(director.play(["event-click"]), 0)
        XCTAssertEqual(misses, [.unplayable(click, url("click.wav"))])
    }

    func testEveryFiringIsReportedEitherWay() {
        let factory = FakeSoundFactory()
        let director = makeDirector(factory: factory, bind: [click])
        var played: [String] = []
        var misses: [KinetoAudioDirector.Miss] = []
        director.onPlay = { played.append($0.name) }
        director.onMiss = { misses.append($0) }

        director.play(["event-click", "event-gone", "event-click"])

        // A log built on these two must be able to tell "nothing fired" from
        // "everything fired and nothing was bound".
        XCTAssertEqual(played, ["ClickSound", "ClickSound"])
        XCTAssertEqual(misses, [.unknownEvent("event-gone")])
    }

    func testOneMissDoesNotStopTheRest() {
        let factory = FakeSoundFactory()
        let director = makeDirector(factory: factory, bind: [click])

        // The machine keeps running, so the sounds after a miss still play.
        XCTAssertEqual(director.play(["event-gone", "event-click"]), 1)
        XCTAssertEqual(factory.made.count, 1)
    }

    func testStopAllSilencesEverythingAndLeavesTheDirectorUsable() {
        let factory = FakeSoundFactory()
        let director = makeDirector(factory: factory, bind: [click])
        director.play(["event-click", "event-click"])

        director.stopAll()

        XCTAssertEqual(director.activeSoundCount, 0)
        XCTAssertTrue(factory.made.allSatisfy { $0.isStopped })
        XCTAssertEqual(director.play(["event-click"]), 1)
    }

    func testBindingMatchingFilesReportsWhatItCouldNotPlace() {
        var library = KinetoAudioLibrary()

        // `pop` carries no sourceName, and this bundle ships no click.wav, so
        // both come back unresolved rather than silently missing later.
        let unresolved = library.bindMatchingFiles(
            for: [click, pop],
            in: Bundle(for: KinetoAudioTests.self)
        )

        XCTAssertEqual(unresolved.map(\.id), ["event-click", "event-pop"])
    }

    // MARK: - Helpers

    private func makeDirector(
        factory: KinetoSoundFactory,
        bind events: [KinetoPlayer.AudioEvent]
    ) -> KinetoAudioDirector {
        var library = KinetoAudioLibrary()
        for event in events {
            library.bind(eventID: event.id, to: url(event.sourceName ?? "\(event.id).wav"))
        }
        return KinetoAudioDirector(events: events, library: library, factory: factory)
    }
}

// MARK: - Fakes

private final class FakeSound: KinetoSound {
    let url: URL
    private(set) var isPlaying = false
    private(set) var isStopped = false
    var volume: Double = 1
    var seekedTo: TimeInterval?
    /// What this sound claims its playhead is, so a test can stage drift.
    var reportedPosition: TimeInterval?

    init(url: URL) {
        self.url = url
    }

    func play() {
        isPlaying = true
    }

    func stop() {
        isPlaying = false
        isStopped = true
    }

    func setVolume(_ volume: Double) {
        self.volume = volume
    }

    func seek(to position: TimeInterval) {
        seekedTo = position
    }

    var currentPosition: TimeInterval? {
        reportedPosition
    }
}

private final class FakeSoundFactory: KinetoSoundFactory {
    var refuses = false
    private(set) var made: [FakeSound] = []

    func makeSound(for url: URL) -> KinetoSound? {
        if refuses {
            return nil
        }
        let sound = FakeSound(url: url)
        made.append(sound)
        return sound
    }
}

/// Issue #111 — the introspection a reference app needs to *show* a document
/// rather than drive a known one, against a real `.kvk` so the record parsing
/// is exercised end to end.
final class KinetoIntrospectionTests: XCTestCase {
    private func loadFixture() throws -> KinetoPlayer {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "pointer_binding", withExtension: "kvk"),
            "the fixture must be bundled with the test target"
        )
        return try KinetoPlayer(contentsOf: url)
    }

    func testEnumeratesTheDocumentsMachines() throws {
        let player = try loadFixture()

        let machines = player.stateMachines()

        XCTAssertFalse(machines.isEmpty, "the fixture carries a state machine")
        XCTAssertFalse(machines[0].id.isEmpty)
        XCTAssertFalse(machines[0].name.isEmpty)
    }

    func testEnumeratesAMachinesInputsWithTheirKinds() throws {
        let player = try loadFixture()
        let machine = try XCTUnwrap(player.stateMachines().first)

        let inputs = player.inputs(in: machine.id)

        XCTAssertFalse(inputs.isEmpty, "the fixture drives an input")
        for input in inputs {
            XCTAssertFalse(input.id.isEmpty)
            XCTAssertFalse(input.name.isEmpty)
            // Every kind must map onto one this build knows; an unknown one is
            // dropped by the parser, so a populated list proves the mapping.
            XCTAssertTrue(
                [.bool, .trigger, .number].contains(input.kind)
            )
        }
    }

    func testAMachineNobodyCarriesHasNoInputs() throws {
        let player = try loadFixture()

        XCTAssertTrue(player.inputs(in: "no-such-machine").isEmpty)
    }

    func testClipsAndSoundsAnswerEvenWhenTheDocumentHasNone() throws {
        let player = try loadFixture()

        // This fixture predates sounds entirely: the right answer is an empty
        // list, not a crash and not an error.
        XCTAssertTrue(player.audioEvents().isEmpty)
        XCTAssertTrue(player.takeFiredAudioEvents().isEmpty)
        // Clips may or may not be present; what matters is that it answers.
        _ = player.clips()
    }

    func testADirectorBuiltFromAPlayerKnowsItsCatalogue() throws {
        let player = try loadFixture()

        let director = KinetoAudioDirector(player: player)

        // No sounds in this document, so every firing is an unknown event
        // rather than a silent no-op that hides a wiring mistake.
        var misses: [KinetoAudioDirector.Miss] = []
        director.onMiss = { misses.append($0) }
        XCTAssertEqual(director.play(["event-click"]), 0)
        XCTAssertEqual(misses, [.unknownEvent("event-click")])
    }
}

/// Issue #111 — timeline audio converges on the playhead rather than being
/// triggered. Every rule below is what keeps runtime playback and the editor's
/// preview telling the same story about a document.
final class KinetoTimelineAudioTests: XCTestCase {
    private func clip(
        track: String = "track-1",
        file: String = "voice.wav",
        position: Double = 0,
        volume: Double = 1
    ) -> KinetoPlayer.ActiveAudioClip {
        KinetoPlayer.ActiveAudioClip(
            trackID: track,
            assetID: "asset-1",
            fileName: file,
            sourcePosition: position,
            volume: volume
        )
    }

    /// A stop that the next frame undoes is not a stop. `sync` restarted the
    /// clip immediately, so the reference app's stop button silenced the
    /// timeline for one frame (issue #111).
    func testStopAllKeepsTheTimelineSilentAcrossLaterSyncs() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory)

        timeline.sync([clip()])
        XCTAssertEqual(timeline.activeTrackIDs, ["track-1"])

        timeline.stopAll()
        timeline.sync([clip()])

        XCTAssertTrue(
            timeline.activeTrackIDs.isEmpty,
            "the clip must not restart while the timeline is stopped"
        )
        XCTAssertEqual(factory.made.count, 1, "and no new voice is made for it")
    }

    /// Playing again has to undo the stop, or the button is a one-way door.
    func testEnablingAgainResumesTheTimeline() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory)
        timeline.sync([clip()])
        timeline.stopAll()

        timeline.isEnabled = true
        timeline.sync([clip()])

        XCTAssertEqual(timeline.activeTrackIDs, ["track-1"])
    }

    /// Pause is the other half: silence now, no flag left behind, so a host
    /// that pauses per frame does not have to re-enable anything.
    func testPauseAllLeavesTheTimelineReadyToResume() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory)
        timeline.sync([clip()])

        timeline.pauseAll()
        XCTAssertTrue(timeline.activeTrackIDs.isEmpty)

        timeline.sync([clip()])
        XCTAssertEqual(timeline.activeTrackIDs, ["track-1"])
    }

    /// Moving a sounding player's playhead clicks. A jump big enough to need
    /// one is almost always a loop, where stopping first and starting again is
    /// both cleaner and what the ear expects.
    func testAJumpRestartsTheVoiceRatherThanSeekingItWhileSounding() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory)
        timeline.sync([clip(position: 8)])
        let voice = factory.made[0]
        voice.seekedTo = nil
        voice.reportedPosition = 8

        // The animation looped: the runtime's position fell back to the start.
        timeline.sync([clip(position: 0)])

        XCTAssertEqual(voice.seekedTo, 0)
        XCTAssertTrue(voice.isStopped, "the voice is stopped before it is moved")
        XCTAssertTrue(voice.isPlaying, "and started again afterwards")
    }

    /// Two clocks that run independently drift apart forever. Re-seeking on
    /// that drift clicks periodically; a fraction of a second of slip does not.
    func testOrdinaryDriftIsLeftAlone() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory)
        timeline.sync([clip(position: 4)])
        let voice = factory.made[0]
        // Creating a voice always seeks it to its start; clear that so the
        // assertion is about drift and nothing else.
        voice.seekedTo = nil
        voice.reportedPosition = 4.3

        timeline.sync([clip(position: 4.6)])

        XCTAssertNil(
            voice.seekedTo,
            "a third of a second of slip is inaudible, and used to click"
        )
    }

    private func makeTimeline(
        _ factory: FakeSoundFactory,
        bindFile: String? = "voice.wav"
    ) -> KinetoTimelineAudio {
        var library = KinetoAudioLibrary()
        if let bindFile {
            library.bind(sourceName: bindFile, to: URL(fileURLWithPath: "/tmp/\(bindFile)"))
        }
        return KinetoTimelineAudio(library: library, factory: factory)
    }

    func testStartsAClipThatBecomesAudible() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory)

        timeline.sync([clip(position: 0.5, volume: 0.8)])

        XCTAssertEqual(timeline.activeTrackIDs, ["track-1"])
        let sound = try? XCTUnwrap(factory.made.first)
        XCTAssertEqual(sound?.isPlaying, true)
        // It starts *where the playhead is*, not at the beginning: a clip
        // scrubbed into must not replay its opening.
        XCTAssertEqual(sound?.seekedTo, 0.5)
        XCTAssertEqual(sound?.volume, 0.8)
    }

    func testDoesNotRestartAClipThatIsStillAudible() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory)

        timeline.sync([clip(position: 0.5)])
        timeline.sync([clip(position: 0.6)])
        timeline.sync([clip(position: 0.7)])

        // One voice for the life of the clip. Restarting every frame is the
        // failure this whole design exists to avoid.
        XCTAssertEqual(factory.made.count, 1)
    }

    func testStopsAClipThatFallsSilent() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory)

        timeline.sync([clip()])
        timeline.sync([])

        XCTAssertTrue(timeline.activeTrackIDs.isEmpty)
        XCTAssertEqual(factory.made.first?.isStopped, true)
    }

    func testSeeksOnlyWhenPlaybackHasDrifted() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory)
        timeline.sync([clip(position: 0)])
        let sound = factory.made[0]
        sound.reportedPosition = 0.30
        sound.seekedTo = nil

        // Within tolerance: ordinary playback drift, left alone.
        timeline.sync([clip(position: 0.32)])
        XCTAssertNil(sound.seekedTo)

        // A playhead jump: far enough that it has to be corrected.
        timeline.sync([clip(position: 5.0)])
        XCTAssertEqual(sound.seekedTo, 5.0)
    }

    func testAClipWithNoFileIsReportedOnce() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory, bindFile: nil)
        var missing: [String] = []
        timeline.onMissingFile = { missing.append($0.fileName) }

        timeline.sync([clip()])

        XCTAssertTrue(factory.made.isEmpty)
        XCTAssertEqual(missing, ["voice.wav"])
    }

    func testStopAllSilencesEverything() {
        let factory = FakeSoundFactory()
        let timeline = makeTimeline(factory)
        timeline.sync([clip()])

        timeline.stopAll()

        XCTAssertTrue(timeline.activeTrackIDs.isEmpty)
        XCTAssertEqual(factory.made.first?.isStopped, true)
    }
}
