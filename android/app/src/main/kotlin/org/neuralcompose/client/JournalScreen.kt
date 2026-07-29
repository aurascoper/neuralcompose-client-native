// M6 Journal slice: rendered PURELY from the core's AudioSnapshot. The shell
// contributes only platform I/O (permission launcher, recorder, player).

package org.neuralcompose.client

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.lifecycle.viewmodel.compose.viewModel
import uniffi.neuralcompose_mobile_core.RecordingPhase

private fun phaseLabel(p: RecordingPhase): String = when (p) {
    is RecordingPhase.Idle -> "IDLE"
    is RecordingPhase.PermissionDenied -> "PERMISSION DENIED"
    is RecordingPhase.Ready -> "READY"
    is RecordingPhase.Recording -> "RECORDING"
    is RecordingPhase.Persisting -> "PERSISTING"
    is RecordingPhase.Recorded -> "RECORDED"
    is RecordingPhase.Playing -> "PLAYING"
    is RecordingPhase.Interrupted -> "INTERRUPTED"
    is RecordingPhase.Failed -> "FAILED: ${p.reason}"
}

@Composable
fun JournalScreen(model: JournalViewModel = viewModel()) {
    val context = LocalContext.current
    val snap by model.state.collectAsState()
    val phase = snap.phase

    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted -> model.onPermissionResult(granted) }

    LaunchedEffect(Unit) {
        val granted = ContextCompat.checkSelfPermission(
            context, Manifest.permission.RECORD_AUDIO,
        ) == PackageManager.PERMISSION_GRANTED
        if (granted) model.onPermissionResult(true)
        else permissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
    }

    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row {
            Text("Journal", fontSize = 28.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.weight(1f))
            Text(
                phaseLabel(phase),
                fontSize = 12.sp,
                fontWeight = FontWeight.Bold,
                fontFamily = FontFamily.Monospace,
                modifier = Modifier
                    .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(50))
                    .padding(horizontal = 10.dp, vertical = 4.dp),
            )
        }

        when (phase) {
            is RecordingPhase.PermissionDenied -> Text(
                "Microphone access denied — voice entries are unavailable. " +
                    "Entries below remain local to this device.",
                color = Color(0xFFD0342C),
            )
            is RecordingPhase.Failed -> Button(onClick = { model.acknowledgeFailure() }) {
                Text("Persist failed (${phase.reason}) — tap to acknowledge")
            }
            else -> Unit
        }

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val recording = phase is RecordingPhase.Recording
            Button(
                onClick = { if (recording) model.stopRecording() else model.startRecording() },
                enabled = phase is RecordingPhase.Ready ||
                    phase is RecordingPhase.Recorded || recording,
                colors = ButtonDefaults.buttonColors(
                    containerColor = if (recording) Color(0xFFD0342C) else Color(0xFFB3403A),
                ),
            ) {
                Text(if (recording) "■ Stop" else "● Record")
            }
            val playing = phase is RecordingPhase.Playing
            Button(
                onClick = { if (playing) model.stopPlayback() else model.playLatest() },
                enabled = playing || (phase is RecordingPhase.Recorded && snap.manifests.isNotEmpty()),
            ) {
                Text(if (playing) "■ Stop" else "▶ Play latest")
            }
        }

        Text(
            "${snap.manifests.size} entr${if (snap.manifests.size == 1) "y" else "ies"} on this device (local only)",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            fontSize = 13.sp,
        )

        LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(snap.manifests.reversed()) { m ->
                Column(
                    Modifier
                        .fillMaxWidth()
                        .background(Color(0x14808080), RoundedCornerShape(8.dp))
                        .padding(10.dp),
                ) {
                    Text(
                        "${m.durationMs / 1000u}.${(m.durationMs % 1000u) / 100u}s · ${m.byteSize} B · ${m.format}",
                        fontWeight = FontWeight.SemiBold,
                        fontSize = 14.sp,
                    )
                    Text(
                        "sha256 ${m.sha256Hex.take(16)}…",
                        fontFamily = FontFamily.Monospace,
                        fontSize = 11.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}
