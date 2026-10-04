import SwiftUI

struct ExpandedReader: View {
    let controller: PanelController
    let style: ReadingStyle

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            ReadingView(model: controller.model, style: style)
            PlayerBar(model: controller.model, theme: style.theme)
        }
    }
}
