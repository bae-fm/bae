import SwiftUI

/// Reads SVG path data's commands and numbers left to right.
struct SVGPathReader {
    enum Token {
        case command(Character)
        /// Numbers follow with no letter: the last command again.
        case numbers
        case end
    }

    private let characters: [Character]
    private var position = 0

    init(_ data: String) {
        characters = Array(data)
    }

    mutating func nextToken() -> Token {
        skipSeparators()
        guard position < characters.count else { return .end }
        let character = characters[position]
        if character.isLetter {
            position += 1
            return .command(character)
        }
        return .numbers
    }

    mutating func number() -> Double {
        skipSeparators()
        let start = position
        if position < characters.count, "+-".contains(characters[position]) {
            position += 1
        }
        var seenPoint = false
        while position < characters.count {
            let character = characters[position]
            if character.isASCII, character.isNumber {
                position += 1
            }
            else if character == ".", !seenPoint {
                seenPoint = true
                position += 1
            }
            else {
                break
            }
        }
        guard let value = Double(String(characters[start..<position])) else {
            preconditionFailure(
                "SVG path data has no number at \(start): \(String(characters))"
            )
        }
        return value
    }

    mutating func point(from origin: CGPoint) -> CGPoint {
        CGPoint(x: origin.x + number(), y: origin.y + number())
    }

    private mutating func skipSeparators() {
        while position < characters.count,
            characters[position] == " " || characters[position] == ","
        {
            position += 1
        }
    }
}
