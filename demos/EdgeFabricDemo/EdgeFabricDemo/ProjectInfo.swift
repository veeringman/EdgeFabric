import Foundation

struct DemoCard: Identifiable {
    let id = UUID()
    let title: String
    let icon: String
    let detail: String
}

enum ProjectInfo {
    static let version = "0.1"
    static let buildProfile = "Local Demo"

    static let demoCards: [DemoCard] = [
        DemoCard(
            title: "iOS Ready",
            icon: "iphone.gen3",
            detail: "Targets the booted simulator so EdgeFabric can verify local Apple platform delivery."
        ),
        DemoCard(
            title: "Edge Native",
            icon: "point.3.connected.trianglepath.dotted",
            detail: "Lives inside this repository so build, package, and deploy flows stay reproducible."
        ),
        DemoCard(
            title: "SwiftUI",
            icon: "swift",
            detail: "Uses a minimal SwiftUI app structure with no external dependencies or asset requirements."
        ),
    ]
}