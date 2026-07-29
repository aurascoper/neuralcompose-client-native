// M6 Journal slice (iOS): rendered PURELY from the core's AudioSnapshot.

import NeuralComposeCore
import SwiftUI

struct JournalScreen: View {
    @StateObject private var model = JournalModel()

    private var phaseLabel: String {
        switch model.snapshot.phase {
        case .idle: return "IDLE"
        case .permissionDenied: return "PERMISSION DENIED"
        case .ready: return "READY"
        case .recording: return "RECORDING"
        case .persisting: return "PERSISTING"
        case .recorded: return "RECORDED"
        case .playing: return "PLAYING"
        case .interrupted: return "INTERRUPTED"
        case .failed(let reason): return "FAILED: \(reason)"
        }
    }

    private var isRecording: Bool {
        if case .recording = model.snapshot.phase { return true }
        return false
    }

    private var isPlaying: Bool {
        if case .playing = model.snapshot.phase { return true }
        return false
    }

    private var recordEnabled: Bool {
        switch model.snapshot.phase {
        case .ready, .recorded, .recording: return true
        default: return false
        }
    }

    /// Playback is permission-independent: legal from Idle/PermissionDenied/
    /// Ready/Recorded when the latest entry exists and passes integrity.
    private var playEnabled: Bool {
        guard let latest = model.snapshot.manifests.last,
            !model.invalidIds.contains(latest.id)
        else { return false }
        switch model.snapshot.phase {
        case .idle, .permissionDenied, .ready, .recorded: return true
        default: return false
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text("Journal").font(.largeTitle.bold())
                    Spacer()
                    Text(phaseLabel)
                        .font(.caption.bold()).monospaced()
                        .padding(.horizontal, 10).padding(.vertical, 4)
                        .background(Capsule().strokeBorder(.secondary))
                }

                if let err = model.manifestError {
                    Text(err)
                        .font(.callout.weight(.semibold))
                        .foregroundStyle(.red)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(8)
                        .background(RoundedRectangle(cornerRadius: 8).strokeBorder(.red))
                }

                if case .permissionDenied = model.snapshot.phase {
                    Text(
                        "Microphone access denied — voice entries are unavailable. Existing entries can still be played; everything stays local."
                    )
                    .foregroundStyle(.red)
                }

                if case .failed(let reason) = model.snapshot.phase {
                    Button("Persist failed (\(reason)) — tap to acknowledge") {
                        model.acknowledgeFailure()
                    }
                }

                HStack(spacing: 10) {
                    Button(isRecording ? "■ Stop" : "● Record") { model.toggleRecord() }
                        .buttonStyle(.borderedProminent)
                        .tint(isRecording ? .red : Color(red: 0.7, green: 0.25, blue: 0.23))
                        .disabled(!recordEnabled)
                    Button(isPlaying ? "■ Stop" : "▶ Play latest") { model.togglePlay() }
                        .buttonStyle(.borderedProminent)
                        .disabled(!isPlaying && !playEnabled)
                }

                Text(
                    "\(model.snapshot.manifests.count) entr\(model.snapshot.manifests.count == 1 ? "y" : "ies") on this device (local only)"
                )
                .font(.caption).foregroundStyle(.secondary)

                ForEach(model.snapshot.manifests.reversed(), id: \.id) { m in
                    let bad = model.invalidIds.contains(m.id)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(
                            "\(Double(m.durationMs) / 1000.0, specifier: "%.1f")s · \(m.byteSize) B · \(m.format)"
                        )
                        .font(.subheadline.weight(.semibold))
                        Text("sha256 \(String(m.sha256Hex.prefix(16)))…")
                            .font(.caption2).monospaced().foregroundStyle(.secondary)
                        if bad {
                            Text(
                                "INTEGRITY ERROR — audio missing or does not match manifest; not playable"
                            )
                            .font(.caption2.bold())
                            .foregroundStyle(.red)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
                    .background(
                        RoundedRectangle(cornerRadius: 8)
                            .fill(bad ? Color.red.opacity(0.12) : Color.gray.opacity(0.12)))
                }
            }
            .padding()
        }
        .onAppear { model.start() }
    }
}
