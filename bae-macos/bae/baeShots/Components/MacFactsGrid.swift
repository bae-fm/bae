import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacFactsGrid`: a file's facts, fewer facts, a value that wraps, and
    /// an album's credits in the wide look.
    static let macFactsGridScenes: [ShotScene] = [
        component(
            "mac.component.facts-grid.file-facts",
            width: 580,
            height: 110
        ) {
            MacFactsGrid {
                MacFact("Folder") {
                    Text(verbatim: "Mara Quill/Low Tide Hours")
                }
                MacFact("Kind") { Text(verbatim: "Audio") }
                MacFact("Format") { Text(verbatim: "FLAC 16/44.1") }
                MacFact("Size") { Text(verbatim: "34.1 MB").monospacedDigit() }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        },
        component("mac.component.facts-grid.some-facts", width: 580, height: 70)
        {
            MacFactsGrid {
                MacFact("Kind") { Text(verbatim: "Rip log") }
                MacFact("Size") { Text(verbatim: "6 KB").monospacedDigit() }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        },
        component(
            "mac.component.facts-grid.long-value",
            width: 580,
            height: 100
        ) {
            MacFactsGrid {
                MacFact("Folder") {
                    Text(
                        verbatim:
                            "The Slow Orchards/Songs for the Slow Orchards, Recorded Live at the Harbor Hall/Disc 2"
                    )
                }
                MacFact("Size") { Text(verbatim: "98.4 MB").monospacedDigit() }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        },
        component("mac.component.facts-grid.wide", width: 580, height: 90) {
            MacFactsGrid(wide: true) {
                MacFact("Producer") { Text(verbatim: "Ines Varga") }
                MacFact("Engineer") { Text(verbatim: "Tomas Brell") }
                MacFact("Arranger") {
                    Text(verbatim: "Odile Fen (tracks 2, 5)")
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        },
    ]
}
