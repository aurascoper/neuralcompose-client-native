// The one working M4 slice: EEG stream status rendered PURELY from the core's
// Presentation + ChannelSnapshot. Labels/banners come from the core's English
// formatters (semantic state is available for localized shells later).

import NeuralComposeCore
import SwiftUI

struct EEGScreen: View {
    @StateObject private var model = EEGStreamModel(
        url: URL(string: "ws://127.0.0.1:8787/api/eeg/stream")!)

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

                ForEach(Array(zip(["TP9", "AF7", "AF8", "TP10"], model.snapshot.channels)),
                        id: \.0) { name, values in
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
            .padding()
        }
        .onAppear { model.start() }
        .onDisappear { model.stop() }
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
