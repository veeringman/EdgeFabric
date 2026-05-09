import SwiftUI

struct ContentView: View {
    private let cards = ProjectInfo.demoCards

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    hero

                    ForEach(cards) { card in
                        VStack(alignment: .leading, spacing: 8) {
                            Label(card.title, systemImage: card.icon)
                                .font(.headline)
                            Text(card.detail)
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(16)
                        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    }
                }
                .padding(20)
            }
            .background(
                LinearGradient(
                    colors: [Color.orange.opacity(0.22), Color.blue.opacity(0.18), Color.black.opacity(0.05)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
                .ignoresSafeArea()
            )
            .navigationTitle("EdgeFabric Demo")
        }
    }

    private var hero: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Build once, ship to edge.")
                .font(.system(size: 32, weight: .bold, design: .rounded))
            Text("This demo app exists to validate local simulator deployment from EdgeFabric on macOS.")
                .font(.body)
                .foregroundStyle(.secondary)

            HStack(spacing: 12) {
                stat(title: "Profile", value: ProjectInfo.buildProfile)
                stat(title: "Target", value: "iOS Simulator")
                stat(title: "Version", value: ProjectInfo.version)
            }
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white.opacity(0.72), in: RoundedRectangle(cornerRadius: 24, style: .continuous))
    }

    private func stat(title: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title.uppercased())
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.callout.weight(.semibold))
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(Color.black.opacity(0.06), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

#Preview {
    ContentView()
}