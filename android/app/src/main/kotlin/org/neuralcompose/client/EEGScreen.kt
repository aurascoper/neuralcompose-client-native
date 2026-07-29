// The working M5-B slice (port of ios/.../EEGScreen.swift): rendered PURELY
// from the core's Presentation + ChannelSnapshot; labels/banners come from
// the core's English formatters.
//
// Plus the Muse golden-capture controls: endpoint, record/stop, and the
// published-recording list with replay verdicts straight from the core's
// verifier — this screen never decides whether a capture is valid.

package org.neuralcompose.client

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import uniffi.neuralcompose_mobile_core.BridgeLocality
import uniffi.neuralcompose_mobile_core.StreamTone
import uniffi.neuralcompose_mobile_core.formatBannerEn
import uniffi.neuralcompose_mobile_core.formatLabelEn

private val CHANNEL_NAMES = listOf("TP9", "AF7", "AF8", "TP10")

private fun toneColor(tone: StreamTone): Color = when (tone) {
    StreamTone.OK -> Color(0xFF2E9E4F)
    StreamTone.STALE, StreamTone.CONNECTING -> Color(0xFFE08A00)
    StreamTone.DOWN -> Color(0xFFD0342C)
}

@Composable
fun EEGScreen(model: EEGStreamViewModel = viewModel()) {
    LaunchedEffect(Unit) { model.start() }
    val ui by model.state.collectAsState()
    val cap by model.capture.collectAsState()
    val tone = toneColor(ui.presentation.tone)

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("EEG Stream", fontSize = 28.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.weight(1f))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .border(1.dp, tone, RoundedCornerShape(50))
                    .padding(horizontal = 10.dp, vertical = 4.dp),
            ) {
                Box(
                    Modifier
                        .size(8.dp)
                        .background(tone, CircleShape),
                )
                Spacer(Modifier.size(6.dp))
                Text(
                    formatLabelEn(ui.presentation),
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Bold,
                    fontFamily = FontFamily.Monospace,
                )
            }
        }

        Text(
            "${ui.snapshot.received} samples received",
            fontSize = 13.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        formatBannerEn(ui.presentation)?.let { banner ->
            Text(
                banner,
                fontSize = 15.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier
                    .fillMaxWidth()
                    .border(1.dp, tone, RoundedCornerShape(8.dp))
                    .background(tone.copy(alpha = 0.12f), RoundedCornerShape(8.dp))
                    .padding(10.dp),
            )
        }

        EndpointControls(cap, model)
        CaptureControls(cap, model)

        ui.snapshot.channels.forEachIndexed { i, values ->
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(CHANNEL_NAMES[i], fontSize = 12.sp, fontWeight = FontWeight.Bold)
                Sparkline(
                    values = values.takeLast(256),
                    color = if (CHANNEL_NAMES[i].startsWith("AF")) Color(0xFF2E9E4F)
                    else Color(0xFF2B6FD9),
                )
            }
        }

        RecordingList(cap, model)
    }
}

@Composable
private fun EndpointControls(cap: CaptureUiState, model: EEGStreamViewModel) {
    // The draft is screen state; the ViewModel only ever sees an applied
    // endpoint, so a half-typed address never becomes the live socket.
    var draft by rememberSaveable(cap.endpoint) { mutableStateOf(cap.endpoint) }

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("Bridge endpoint", fontSize = 13.sp, fontWeight = FontWeight.Bold)
        OutlinedTextField(
            value = draft,
            onValueChange = { draft = it },
            singleLine = true,
            label = { Text("ws:// host : port / path") },
            modifier = Modifier.fillMaxWidth(),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = { model.applyEndpoint(draft) }) { Text("Connect") }
            TextButton(onClick = { model.reconnect() }) { Text("Reconnect") }
        }
        Text(
            "Live: ${cap.endpoint}  •  " +
                when (cap.locality) {
                    BridgeLocality.LOCAL_NETWORK -> "localNetwork"
                    BridgeLocality.REMOTE_ENDPOINT -> "remoteEndpoint"
                },
            fontSize = 11.sp,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (cap.cleartextBlocked) {
            Warning(
                "Cleartext blocked for this host by the app's network security " +
                    "policy. Add it to res/xml/network_security_config.xml or use wss://.",
            )
        }
    }
}

@Composable
private fun CaptureControls(cap: CaptureUiState, model: EEGStreamViewModel) {
    val activeCapture = cap.active
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("Golden capture", fontSize = 13.sp, fontWeight = FontWeight.Bold)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(
                onClick = { model.startRecording() },
                enabled = activeCapture == null,
            ) { Text("Start recording") }
            Button(
                onClick = { model.stopRecording() },
                enabled = activeCapture != null,
                colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFD0342C)),
            ) { Text("Stop") }
        }
        if (activeCapture != null) {
            Text(
                "REC ${activeCapture.recordingId}  •  " +
                    "${activeCapture.messagesReceived} msgs  •  " +
                    "${activeCapture.acceptedSampleCount} samples  •  " +
                    "${activeCapture.elapsedMs} ms",
                fontSize = 12.sp,
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Bold,
                color = Color(0xFFD0342C),
            )
        }
        cap.notice?.let {
            Text(
                it,
                fontSize = 12.sp,
                fontFamily = FontFamily.Monospace,
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Color(0x14808080), RoundedCornerShape(6.dp))
                    .padding(8.dp),
            )
        }
    }
}

@Composable
private fun RecordingList(cap: CaptureUiState, model: EEGStreamViewModel) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                "Published recordings (${cap.recordings.size})",
                fontSize = 13.sp,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.weight(1f))
            TextButton(onClick = { model.refreshRecordings(null) }) { Text("Rescan") }
        }
        cap.problems.forEach { Warning(it) }
        if (cap.recordings.isEmpty()) {
            Text(
                "None yet. Recordings survive restart; .partial files are never listed.",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        cap.recordings.forEach { rec ->
            val m = rec.manifest
            Column(
                verticalArrangement = Arrangement.spacedBy(4.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .border(1.dp, Color(0x33808080), RoundedCornerShape(8.dp))
                    .padding(10.dp),
            ) {
                Text(m.recordingId, fontSize = 13.sp, fontWeight = FontWeight.Bold)
                Text(
                    "${m.durationMs} ms  •  ${m.acceptedSampleCount} samples  •  " +
                        "${m.messagesReceived} msgs (${m.rejectedMessageCount} rejected)  •  " +
                        "${m.payloadByteSize} bytes",
                    fontSize = 11.sp,
                    fontFamily = FontFamily.Monospace,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (!rec.payloadExists) {
                    Warning("Payload file missing — this manifest cannot be replayed.")
                } else if (rec.payloadFileSize.toULong() != m.payloadByteSize) {
                    Warning(
                        "Payload is ${rec.payloadFileSize} bytes on disk but the " +
                            "manifest claims ${m.payloadByteSize}.",
                    )
                }
                cap.verdicts[rec.id]?.let { verdict ->
                    Text(
                        verdict,
                        fontSize = 12.sp,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Bold,
                        color = if (verdict.startsWith("VERIFIED")) {
                            Color(0xFF2E9E4F)
                        } else {
                            Color(0xFFD0342C)
                        },
                    )
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = { model.verify(rec.id) }) { Text("Verify") }
                    TextButton(onClick = { model.delete(rec.id) }) { Text("Delete") }
                }
            }
        }
    }
}

@Composable
private fun Warning(message: String) {
    Text(
        message,
        fontSize = 12.sp,
        fontWeight = FontWeight.SemiBold,
        color = Color(0xFFD0342C),
        modifier = Modifier
            .fillMaxWidth()
            .border(1.dp, Color(0xFFD0342C), RoundedCornerShape(6.dp))
            .padding(8.dp),
    )
}

@Composable
private fun Sparkline(values: List<Double>, color: Color) {
    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(64.dp)
            .background(Color(0x14808080), RoundedCornerShape(8.dp)),
    ) {
        if (values.size < 2) return@Canvas
        val minV = values.min()
        val maxV = values.max()
        val span = (maxV - minV).coerceAtLeast(1e-9)
        val stepX = size.width / (values.size - 1)
        val path = Path()
        values.forEachIndexed { i, v ->
            val x = i * stepX
            val y = size.height * (1f - ((v - minV) / span).toFloat())
            if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
        }
        drawPath(path, color, style = Stroke(width = 2f))
    }
}
