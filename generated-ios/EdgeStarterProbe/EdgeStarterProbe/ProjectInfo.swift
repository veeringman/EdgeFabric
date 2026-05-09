import Foundation

struct StarterCard: Identifiable {
    let id = UUID()
    let title: String
    let detail: String
    let icon: String
}

enum ProjectInfo {
    static let displayName = "EdgeStarterProbe"
    static let bundleId = "com.edgefabric.samples.edgestarterprobe"
    static let interfaceLabel = "swiftui"
    static let starterCards: [StarterCard] = [
        StarterCard(
            title: "Starter Ready",
            detail: "This project was generated inside EdgeFabric and is ready to open in Xcode or VS Code.",
            icon: "sparkles"
        ),
        StarterCard(
            title: "Signing",
            detail: "No Apple team assigned yet. Update the team id when you are ready for device signing.",
            icon: "person.crop.rectangle"
        ),
        StarterCard(
            title: "Deployment Target",
            detail: "iOS 17.0",
            icon: "iphone.gen3"
        ),
    ]
}
