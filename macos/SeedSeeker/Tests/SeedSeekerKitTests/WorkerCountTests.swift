import Foundation
import XCTest
@testable import SeedSeekerKit

/// The worker count is a preference about this machine — how many search
/// threads its cores can usefully run — rather than a part of the query. These
/// tests pin both halves of that: it is clamped to whatever the machine at hand
/// offers, and it never travels with a query.
final class WorkerCountTests: XCTestCase {
    private func sampleQuery() throws -> SavedQuery {
        SavedQuery(
            requirements: [
                try ItemRequirement(key: 1, item: ItemCatalog.findById("wand_frost"),
                                    upgrade: 2, kind: .wand, upgradeMatch: .atLeast),
            ],
            maximumDepth: 12, requireBlacksmith: true,
            challenges: Challenge.noHerbalism.rawValue)
    }

    /// The ceiling comes from the engine and is the one value the selector may
    /// not exceed. The engine promises at least one worker, so there is always
    /// a valid choice.
    func testEngineReportsAtLeastOneAvailableWorker() {
        XCTAssertGreaterThanOrEqual(EngineInfo.availableWorkers, 1)
    }

    /// No stored preference means the whole machine, not a single thread: a
    /// fresh install searches at full speed without anyone touching a slider.
    func testUnsetPreferenceMeansEveryAvailableCore() {
        XCTAssertEqual(WorkerPersistence.resolve(saved: WorkerPersistence.unset, ceiling: 8), 8)
        XCTAssertEqual(WorkerPersistence.resolve(saved: WorkerPersistence.unset, ceiling: 1), 1)
        // A hand-edited or otherwise nonsensical defaults entry reads as unset
        // rather than as "one worker".
        XCTAssertEqual(WorkerPersistence.resolve(saved: -4, ceiling: 8), 8)
        // A ceiling that somehow arrives below one still yields a usable count.
        XCTAssertEqual(WorkerPersistence.resolve(saved: WorkerPersistence.unset, ceiling: 0), 1)
        XCTAssertEqual(WorkerPersistence.resolve(saved: 4, ceiling: 0), 1)
    }

    /// The saved value may have been chosen on a roomier machine, so loading
    /// clamps it down instead of asking for cores that are not there.
    func testPersistedCountIsClampedIntoRangeOnLoad() {
        XCTAssertEqual(WorkerPersistence.resolve(saved: 16, ceiling: 8), 8)
        XCTAssertEqual(WorkerPersistence.resolve(saved: 8, ceiling: 8), 8)
        XCTAssertEqual(WorkerPersistence.resolve(saved: 3, ceiling: 8), 3)
        XCTAssertEqual(WorkerPersistence.resolve(saved: 1, ceiling: 8), 1)

        XCTAssertEqual(WorkerPersistence.clamp(99, ceiling: 8), 8)
        XCTAssertEqual(WorkerPersistence.clamp(4, ceiling: 8), 4)
        // A clamped choice is always a real one: unlike the stored preference,
        // zero is not "every core" here but the slider's floor.
        XCTAssertEqual(WorkerPersistence.clamp(0, ceiling: 8), 1)
        XCTAssertEqual(WorkerPersistence.clamp(-3, ceiling: 8), 1)
    }

    /// The same round trip `@AppStorage` performs: an absent key reads as the
    /// unset default, and a value saved on a bigger machine comes back clamped.
    func testDefaultsRoundTripStartsUnsetAndLoadsClamped() throws {
        let suite = "WorkerCountTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let key = WorkerPersistence.defaultsKey

        XCTAssertEqual(defaults.integer(forKey: key), WorkerPersistence.unset,
                       "a machine that never touched the selector stores nothing")
        XCTAssertEqual(WorkerPersistence.resolve(saved: defaults.integer(forKey: key), ceiling: 6), 6)

        defaults.set(12, forKey: key)
        XCTAssertEqual(WorkerPersistence.resolve(saved: defaults.integer(forKey: key), ceiling: 6), 6)

        defaults.set(2, forKey: key)
        XCTAssertEqual(WorkerPersistence.resolve(saved: defaults.integer(forKey: key), ceiling: 6), 2)
    }

    /// Everything a query can travel in must stay free of the preference: the
    /// engine's query document, the saved query, a preset, a results export and
    /// a share link. A query opened on another machine has to search the same
    /// seeds there, whatever that machine's core count.
    func testWorkerCountNeverEntersAnythingAQueryTravelsIn() throws {
        let query = try sampleQuery()
        let request = try SearchRequest(
            requirements: query.requirements, maximumDepth: query.maximumDepth,
            requireBlacksmith: query.requireBlacksmith,
            excludeBlacksmithRewards: query.excludeBlacksmithRewards,
            wandmakerQuest: query.wandmakerQuest, challenges: query.challenges)

        let document = try XCTUnwrap(String(data: try QueryDocument.encode(request), encoding: .utf8))
        let saved = try XCTUnwrap(QueryPersistence.encode(query))
        let preset = try XCTUnwrap(PresetPersistence.encode([QueryPreset(name: "Sample", query: query)]))
        let export = ResultsExport.encode(query, seeds: ["AAA-AAA-AAA"], appVersion: "1.0")
        let link = try DeepLink.encodeLink(for: query)

        for (name, text) in [("query document", document), ("saved query", saved),
                             ("preset", preset), ("results export", export)] {
            XCTAssertFalse(text.localizedCaseInsensitiveContains("worker"),
                           "the \(name) must not carry the device-local worker count: \(text)")
            XCTAssertFalse(text.contains(WorkerPersistence.defaultsKey),
                           "the \(name) must not carry the device-local worker count: \(text)")
        }
        // The link is an opaque code, so the guarantee is checked by decoding:
        // it round trips to exactly the query that was shared.
        XCTAssertEqual(try DeepLink.decode(link).maximumDepth, query.maximumDepth)
        XCTAssertEqual(try DeepLink.decode(link).challenges, query.challenges)
    }

    /// A saved query carrying the key anyway — hand-edited defaults, or a
    /// future build that stored it in the wrong place — loads as an ordinary
    /// query and does not write the key back out.
    func testASavedQueryCarryingTheKeyIgnoresIt() throws {
        let legacy = """
        {"requirements":[{"key":1,"upgrade":2,"kind":3,"tier":0,"tierMatch":0,\
        "upgradeMatch":1,"requireUncursed":false}],\
        "maximumDepth":12,"requireBlacksmith":false,"excludeBlacksmithRewards":false,\
        "workerCount":3,"challenges":0}
        """
        let decoded = QueryPersistence.decode(legacy)
        XCTAssertEqual(decoded.requirements.count, 1)
        XCTAssertEqual(decoded.maximumDepth, 12)
        let reencoded = try XCTUnwrap(QueryPersistence.encode(decoded))
        XCTAssertFalse(reencoded.contains(WorkerPersistence.defaultsKey))
    }
}
