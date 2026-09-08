import ImageIO
import SeedSeekerKit
import SwiftUI

/// The same 15×16 upstream feeling frames used by the web scout.
/// Normal floors have no icon; the label is only exposed to accessibility.
struct FloorFeelingSpriteView: View {
    let feeling: FloorFeeling

    private static let atlas: CGImage? = {
        guard let url = Bundle.main.url(forResource: "dungeon-icons", withExtension: "png"),
              let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }()

    var body: some View {
        if feeling != .none,
           let sprite = Self.atlas?.cropping(to: CGRect(x: 16 * feeling.rawValue, y: 64,
                                                        width: 15, height: 16)) {
            Image(decorative: sprite, scale: 1)
                .interpolation(.none)
                .antialiased(false)
                .accessibilityHidden(false)
                .accessibilityLabel(feeling.label)
        }
    }
}
