// Compose shell skeleton mirroring the iOS App.swift: six tabs, EEG
// functional (M5-B slice), others placeholders that will render
// server-reported data — never claiming locality the server didn't report.

package org.neuralcompose.client

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.Arrangement

private data class Tab(val label: String, val short: String)

private val TABS = listOf(
    Tab("Overview", "OV"),
    Tab("EEG", "EEG"),
    Tab("Health", "HP"),
    Tab("Classifier", "ML"),
    Tab("Dialectic", "DL"),
    Tab("Journal", "JR"),
)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                var selected by remember { mutableIntStateOf(1) } // EEG default for the slice
                Scaffold(
                    bottomBar = {
                        NavigationBar {
                            TABS.forEachIndexed { i, tab ->
                                NavigationBarItem(
                                    selected = selected == i,
                                    onClick = { selected = i },
                                    icon = { Text(tab.short, fontSize = 11.sp, fontWeight = FontWeight.Bold) },
                                    label = { Text(tab.label, fontSize = 10.sp) },
                                )
                            }
                        }
                    },
                ) { padding ->
                    Column(Modifier.padding(padding)) {
                        when (selected) {
                            1 -> EEGScreen()
                            5 -> JournalScreen()
                            else -> Placeholder(TABS[selected].label)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun Placeholder(title: String) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(title, fontSize = 28.sp, fontWeight = FontWeight.Bold)
        Text(
            "Will render server-reported data. M5-B slice: see the EEG tab.",
            textAlign = TextAlign.Center,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
