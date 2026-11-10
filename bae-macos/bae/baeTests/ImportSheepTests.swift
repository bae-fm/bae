import AppKit
import SwiftUI
import Testing

@testable import bae

@Suite("Import sheep")
@MainActor
struct ImportSheepTests {
    @Test("Every part of the sheep's drawing is in the asset catalog")
    func partsAreInTheCatalog() {
        for art in SheepArt.allCases {
            #expect(NSImage(named: art.assetName) != nil, "\(art.assetName)")
        }
    }

    @Test("Path data reads arcs, relative moves and implicit repeats")
    func pathData() {
        let disc = SheepShapes.disc.boundingRect
        #expect(abs(disc.minX + 50) < 0.01 && abs(disc.maxX - 50) < 0.01)
        #expect(abs(disc.minY + 50) < 0.01 && abs(disc.maxY - 50) < 0.01)

        // A half circle of radius 40 from (-30,-26) to (-8,-38).
        let shine = SheepShapes.shine.currentPoint
        #expect(shine == CGPoint(x: -8, y: -38))

        let lines = Path(svg: "M10 20l5 5 5-5H0V0z")
        #expect(lines.boundingRect == CGRect(x: 0, y: 0, width: 20, height: 25))
    }

    @Test("A turn of the loop is 57 seconds in eight moments")
    func loopLength() {
        #expect(
            SheepSequence.moments.map(\.duration)
                == [7, 7.5, 7, 7, 7.5, 7.5, 6, 7.5]
        )
        #expect(SheepSequence.duration == 57)
        let (moment, time) = SheepSequence.moment(at: 7 + 7.5 + 1 + 57 * 3)
        #expect(moment.duration == 7)
        #expect(abs(time - 1) < 1e-9)
    }

    @Test("Every part starts and ends each moment at rest, or unseen")
    func momentsJoin() {
        var parts: [SheepPart] = [
            .roll, .rig, .head, .earLeft, .earRight, .features, .cheekLeft,
            .cheekRight, .mouth, .gape, .munch, .eyes, .heart, .arms,
            .surprise(.left), .surprise(.right), .surprise(.top), .blush,
            .flash, .floatingHeart,
        ]
        parts += (0..<5).map(SheepPart.star)
        parts += (0..<4).map(SheepPart.wind)
        for moment in SheepSequence.moments {
            var all = parts
            for index in moment.releases.indices {
                all += [.release(index), .spin(index)]
            }
            for part in all {
                for time in [0, moment.duration] {
                    let pose = moment.pose(of: part, at: time)
                    // A part drawn only while it moves is unseen at the ends,
                    // wherever it waits.
                    let unseen = part.rest.opacity == 0
                    #expect(
                        unseen ? pose.opacity == 0 : pose.isNear(part.rest),
                        "\(part) at \(time) of a \(moment.duration) s moment: \(pose)"
                    )
                }
            }
        }
    }

    @Test("Moves follow their keys and the easing between them")
    func tracks() {
        let track = SheepTrack(
            [
                SheepKey(0, .rest, .linear),
                SheepKey(1, .at(10, 0, 90, 2), .hold),
                SheepKey(2, .rest.opacity(0), .linear),
            ],
            duration: 3
        )
        #expect(track.pose(at: 0.5) == .at(5, 0, 45, 1.5))
        // A held key stays put until the next one.
        #expect(track.pose(at: 1.9) == .at(10, 0, 90, 2))
        #expect(track.pose(at: 2) == .rest.opacity(0))
        // A track that stops short goes back to its first pose by the end.
        #expect(track.pose(at: 2.5).isNear(.rest.opacity(0)))
        #expect(track.pose(at: 3) == .rest)
        for ease: SheepEase in [.inOut, .out, .in, .linear, .back, .snap] {
            #expect(abs(ease.progress(0)) < 1e-6)
            #expect(abs(ease.progress(1) - 1) < 1e-6)
        }
    }

    @Test("The sheep draws something, still and mid-moment")
    func draws() throws {
        let size = CGSize(width: 462, height: 383.5)
        let frames: [(SheepMoment, Double)] = [
            (SheepSequence.still, 0),
            (SheepSequence.moments[0], 2.0),
            (SheepSequence.moments[4], 3.2),
        ]
        for (moment, time) in frames {
            let renderer = ImageRenderer(
                content: Canvas { context, canvas in
                    var context = context
                    ImportSheepPainter(moment: moment, time: time)
                        .draw(into: &context, size: canvas)
                }
                .frame(width: size.width, height: size.height)
            )
            let image = try #require(renderer.cgImage)
            let pixels = NSBitmapImageRep(cgImage: image)
            var seen = 0
            for y in stride(from: 0, to: pixels.pixelsHigh, by: 4) {
                for x in stride(from: 0, to: pixels.pixelsWide, by: 4)
                where (pixels.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.5 {
                    seen += 1
                }
            }
            #expect(
                seen > 2000,
                "\(moment.duration) s moment at \(time): \(seen)"
            )
        }
    }
}

extension SheepPose {
    fileprivate func isNear(_ other: SheepPose) -> Bool {
        abs(x - other.x) < 1e-6 && abs(y - other.y) < 1e-6
            && abs(
                turn.truncatingRemainder(dividingBy: 360)
                    - other.turn.truncatingRemainder(dividingBy: 360)
            ) < 1e-6
            && abs(scaleX - other.scaleX) < 1e-6
            && abs(scaleY - other.scaleY) < 1e-6
            && abs(opacity - other.opacity) < 1e-6
    }
}
