// The working M5-B slice (port of ios/.../EEGScreen.swift): rendered PURELY
// from the core's Presentation + ChannelSnapshot; labels/banners come from
// the core's English formatters.

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
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
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
    }
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
