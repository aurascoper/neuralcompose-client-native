// The one working M4 slice: EEG stream status rendered PURELY from the core's
// Presentation + ChannelSnapshot. Labels/banners come from the core's English
// formatters (semantic state is available for localized shells later).
//
// Plus the Muse golden-capture gate controls: endpoint, record, and the
// published recordings with per-recording replay verification.

import NeuralComposeCore
import SwiftUI

struct EEGScreen: View {
    @StateObject private var model = EEGStreamModel()

    private var toneColor: Color {
        switch model.presentation.tone {
        case .ok: return .green
        case .stale, .connecting: return .orange
        case .down: return .red
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                Text("\(model.snapshot.received) samples received")
                    .font(.caption).foregroundStyle(.secondary)

                if let banner = formatBannerEn(p: model.presentation) {
                    Text(banner)
                        .font(.callout.weight(.semibold))
                        .frame(maxWidth: .infinity)
                        .padding(10)
                        .background(
                            RoundedRectangle(cornerRadius: 8)
                                .strokeBorder(toneColor)
                                .background(toneColor.opacity(0.15))
                        )
                }

                endpointSection
                captureSection
                channels
                recordingsSection
            }
            .padding()
        }
        .onAppear { model.start() }
        .onDisappear { model.stop() }
    }

    private var header: some View {
        HStack {
            Text("EEG Stream").font(.largeTitle.bold())
            Spacer()
            HStack(spacing: 6) {
                Circle().fill(toneColor).frame(width: 8, height: 8)
                Text(formatLabelEn(p: model.presentation))
                    .font(.caption.bold()).monospaced()
            }
            .padding(.horizontal, 10).padding(.vertical, 4)
            .background(Capsule().strokeBorder(toneColor))
        }
    }

    // MARK: - endpoint

    private var endpointSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Bridge endpoint").font(.headline)
            TextField("ws://host:port/api/eeg/stream", text: $model.endpointText)
                .textFieldStyle(.roundedBorder)
                .font(.callout.monospaced())
                .keyboardType(.URL)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .submitLabel(.go)
                .onSubmit { model.applyEndpoint() }
            HStack {
                Button("Connect") { model.applyEndpoint() }
                    .buttonStyle(.borderedProminent)
                    .disabled(model.isRecording)
                Spacer()
                Text(model.connectedEndpoint)
                    .font(.caption2.monospaced()).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.head)
            }
            if let error = model.endpointError {
                Text(error).font(.caption).foregroundStyle(.red)
            }
            Text("On a physical phone 127.0.0.1 is the phone itself — use the bridge's LAN address.")
                .font(.caption2).foregroundStyle(.tertiary)
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 10).fill(.quaternary.opacity(0.25)))
    }

    // MARK: - capture

    private var captureSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Golden capture").font(.headline)
                Spacer()
                Button(model.isRecording ? "Stop" : "Start recording") {
                    if model.isRecording { model.stopRecording() } else { model.startRecording() }
                }
                .buttonStyle(.borderedProminent)
                .tint(model.isRecording ? .red : .accentColor)
            }

            if let stats = model.captureStats {
                VStack(alignment: .leading, spacing: 2) {
                    Text(stats.recordingId).font(.caption.monospaced().bold())
                    Text(
                        "\(stats.messagesReceived) messages · \(stats.acceptedSampleCount) samples · "
                            + "\(model.elapsedMs / 1000)s"
                    )
                    .font(.caption.monospaced())
                    if let failure = stats.writeFailure {
                        Text("WRITE FAILED — will discard on stop: \(failure)")
                            .font(.caption.bold()).foregroundStyle(.red)
                    }
                    Text("Stream stays connected while recording, even off this tab.")
                        .font(.caption2).foregroundStyle(.tertiary)
                }
            }

            if let message = model.captureMessage {
                Text(message).font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 10).fill(.quaternary.opacity(0.25)))
    }

    // MARK: - channels

    private var channels: some View {
        ForEach(
            Array(zip(["TP9", "AF7", "AF8", "TP10"], model.snapshot.channels)),
            id: \.0
        ) { name, values in
            VStack(alignment: .leading, spacing: 4) {
                Text(name).font(.caption.bold())
                Sparkline(values: Array(values.suffix(256)))
                    .stroke(name.hasPrefix("AF") ? Color.green : Color.blue, lineWidth: 1)
                    .frame(height: 64)
                    .background(
                        RoundedRectangle(cornerRadius: 8).fill(.quaternary.opacity(0.3)))
            }
        }
    }

    // MARK: - recordings

    private var recordingsSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Recordings").font(.headline)
                Spacer()
                Button("Refresh") { model.reloadRecordings() }.font(.caption)
            }

            if model.listing.recordings.isEmpty {
                Text("No published recordings on this device.")
                    .font(.caption).foregroundStyle(.secondary)
            }

            ForEach(model.listing.recordings) { recording in
                RecordingRow(
                    recording: recording,
                    verdict: model.verdicts[recording.id],
                    verify: { model.verify(recording) },
                    delete: { model.delete(recording) })
            }

            if !model.listing.unreadable.isEmpty {
                Text("Unreadable manifests: \(model.listing.unreadable.joined(separator: ", "))")
                    .font(.caption).foregroundStyle(.red)
            }
            if !model.listing.partials.isEmpty {
                Text(
                    "\(model.listing.partials.count) incomplete .partial file(s) from an interrupted run — not recordings."
                )
                .font(.caption2).foregroundStyle(.orange)
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 10).fill(.quaternary.opacity(0.25)))
    }
}

private struct RecordingRow: View {
    let recording: CapturedRecording
    let verdict: String?
    let verify: () -> Void
    let delete: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(recording.id).font(.caption.monospaced().bold())
            Text(
                "\(recording.manifest.durationMs) ms · \(recording.manifest.acceptedSampleCount) samples · "
                    + "\(recording.manifest.payloadByteSize) B · \(recording.manifest.messagesReceived) msgs"
            )
            .font(.caption2.monospaced()).foregroundStyle(.secondary)
            Text(
                "\(recording.manifest.platform) \(recording.manifest.osVersion) · "
                    + "\(CaptureStore.string(for: recording.manifest.bridgeLocality)) · "
                    + "sha \(recording.manifest.payloadSha256Hex.prefix(12))"
            )
            .font(.caption2.monospaced()).foregroundStyle(.tertiary)

            if let note = recording.integrityNote {
                Text(note).font(.caption2.bold()).foregroundStyle(.red)
            }
            if let verdict {
                Text(verdict)
                    .font(.caption2)
                    .foregroundStyle(verdict.hasPrefix("Verified") ? .green : .red)
            }

            HStack {
                Button("Verify", action: verify).buttonStyle(.bordered).font(.caption)
                Button("Delete", role: .destructive, action: delete)
                    .buttonStyle(.bordered).font(.caption)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 8).fill(.quaternary.opacity(0.3)))
    }
}

struct Sparkline: Shape {
    let values: [Double]

    func path(in rect: CGRect) -> Path {
        var path = Path()
        guard values.count > 1 else { return path }
        let minV = values.min() ?? 0
        let maxV = values.max() ?? 1
        let span = max(maxV - minV, 1e-9)
        let stepX = rect.width / CGFloat(values.count - 1)
        for (i, v) in values.enumerated() {
            let x = CGFloat(i) * stepX
            let y = rect.height * (1 - CGFloat((v - minV) / span))
            if i == 0 { path.move(to: CGPoint(x: x, y: y)) } else {
                path.addLine(to: CGPoint(x: x, y: y))
            }
        }
        return path
    }
}
