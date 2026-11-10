import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The sheep on the empty Import screen at a moment of each of its eight
    /// turns of the loop, side by side: the loop itself can't show in a still.
    static let importSheepMoments = ShotScene(
        id: "import-empty-sheep-moments",
        size: CGSize(width: 1280, height: 1040)
    ) {
        let frames: [(moment: Int, time: Double)] = [
            (0, 2.0), (0, 3.3), (1, 1.1), (1, 1.9),
            (1, 2.6), (2, 2.4), (3, 2.0), (4, 2.3),
            (4, 3.2), (5, 1.1), (6, 1.5), (7, 1.2),
        ]
        return ShotStage(
            LazyVGrid(
                columns: Array(
                    repeating: GridItem(.fixed(300), spacing: ThemeSpace.group),
                    count: 4
                ),
                spacing: ThemeSpace.group
            ) {
                ForEach(frames.indices, id: \.self) { index in
                    let frame = frames[index]
                    Canvas { context, size in
                        var context = context
                        ImportSheepPainter(
                            moment: SheepSequence.moments[frame.moment],
                            time: frame.time
                        )
                        .draw(into: &context, size: size)
                    }
                    .frame(width: 300, height: 249)
                }
            }
            .padding(ThemeSpace.page)
            .windowBackground()
        )
    }
}
