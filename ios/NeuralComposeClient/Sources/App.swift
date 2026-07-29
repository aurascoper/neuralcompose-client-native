// SwiftUI shell skeleton mirroring the Expo client's tab structure. Only the
// EEG tab is a working slice in M4; the others are placeholders that will
// render server-reported data (never claiming locality the server didn't
// report).

import SwiftUI

@main
struct NeuralComposeClientApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}

struct ContentView: View {
    var body: some View {
        TabView {
            Placeholder(
                title: "Overview",
                note: "Will render the server-reported pipeline mode + diagnostics."
            )
            .tabItem { Label("Overview", systemImage: "gauge") }

            EEGScreen()
                .tabItem { Label("EEG", systemImage: "waveform.path.ecg") }

            Placeholder(title: "Health", note: "Per-channel health from /api/health.")
                .tabItem { Label("Health", systemImage: "heart") }

            Placeholder(title: "Classifier", note: "Intent + distribution from /api/classifier.")
                .tabItem { Label("Classifier", systemImage: "brain.head.profile") }

            Placeholder(title: "Dialectic", note: "Live Dialectic session (server-gated).")
                .tabItem { Label("Dialectic", systemImage: "bubble.left.and.bubble.right") }

            JournalScreen()
                .tabItem { Label("Journal", systemImage: "square.and.pencil") }
        }
    }
}

struct Placeholder: View {
    let title: String
    let note: String

    var body: some View {
        VStack(spacing: 12) {
            Text(title).font(.largeTitle.bold())
            Text(note).font(.callout).foregroundStyle(.secondary)
                .multilineTextAlignment(.center).padding(.horizontal)
            Text("M4 slice: see the EEG tab").font(.caption).foregroundStyle(.tertiary)
        }
    }
}
