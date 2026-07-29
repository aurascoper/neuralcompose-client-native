// The shell side of the contract (port of ios/.../EEGStreamModel.swift):
// owns the socket and timers, feeds raw frames + MONOTONIC timestamps into
// the Rust core, and renders whatever the core says. It never derives stream
// health from socket state — M5-A semantics: a reopened socket is
// OpenNoData until its own first accepted frame.

package org.neuralcompose.client

import android.os.SystemClock
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import uniffi.neuralcompose_mobile_core.ChannelSnapshot
import uniffi.neuralcompose_mobile_core.MonitorConfig
import uniffi.neuralcompose_mobile_core.Presentation
import uniffi.neuralcompose_mobile_core.ReconnectDecision
import uniffi.neuralcompose_mobile_core.SocketEvent
import uniffi.neuralcompose_mobile_core.StreamMonitor

data class EEGUiState(
    val presentation: Presentation,
    val snapshot: ChannelSnapshot,
)

class EEGStreamViewModel(
    private val url: String = "ws://127.0.0.1:8787/api/eeg/stream",
) : ViewModel() {

    private val monitor = StreamMonitor(
        MonitorConfig(
            keepSamples = 1280u,
            staleAfterMs = 2000uL,
            maxReconnectAttempts = 3u,
            backoffBaseMs = 500uL,
            backoffCapMs = 30000uL,
        ),
    )

    // pingInterval is load-bearing: without it a socket that dies while the
    // process is frozen (screen off) stays a believed-open zombie forever —
    // the core then reports STALE honestly but recovery never starts. Pings
    // surface the dead socket as onFailure → CLOSED → the core's retry ladder.
    private val client = OkHttpClient.Builder()
        .pingInterval(java.time.Duration.ofSeconds(5))
        .build()
    private var socket: WebSocket? = null
    private var pollJob: Job? = null

    private val _state = MutableStateFlow(
        EEGUiState(monitor.presentation(nowMs()), monitor.snapshot()),
    )
    val state: StateFlow<EEGUiState> = _state

    /** Monotonic ms — never wall clock. */
    private fun nowMs(): ULong = SystemClock.elapsedRealtime().toULong()

    fun start() {
        if (pollJob != null) return
        connect()
        pollJob = viewModelScope.launch {
            while (true) {
                _state.value = EEGUiState(monitor.presentation(nowMs()), monitor.snapshot())
                delay(500)
            }
        }
    }

    private fun connect() {
        monitor.onSocketEvent(SocketEvent.CONNECTING, nowMs())
        socket = client.newWebSocket(
            Request.Builder().url(url).build(),
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    monitor.onSocketEvent(SocketEvent.OPENED, nowMs())
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    monitor.onFrame(text, nowMs())
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    handleDisconnect()
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    handleDisconnect()
                }
            },
        )
    }

    private fun handleDisconnect() {
        monitor.onSocketEvent(SocketEvent.CLOSED, nowMs())
        when (val decision = monitor.reconnectDecision()) {
            is ReconnectDecision.RetryAfterMs -> viewModelScope.launch {
                delay(decision.delayMs.toLong())
                connect()
            }
            is ReconnectDecision.GiveUp -> Unit // Error latched; core reports it
        }
    }

    override fun onCleared() {
        pollJob?.cancel()
        socket?.close(1000, "view model cleared")
        client.dispatcher.executorService.shutdown()
    }
}
