package dev.jpcottin.netman.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.jpcottin.netman.graph.Visual
import dev.jpcottin.netman.vpn.VpnState

/** Mode d'affichage d'un panneau : graphe (Canvas) ou liste triée. */
enum class ViewMode { GRAPH, LIST }

/** État complet dont l'écran a besoin (facilite previews et tests). */
data class MainUiState(
    val vpn: VpnState = VpnState.Idle,
    val appGraph: GraphSnapshot = GraphSnapshot(emptyList(), emptyList()),
    val interGraph: GraphSnapshot = GraphSnapshot(emptyList(), emptyList()),
    val appRows: List<NodeRow> = emptyList(),
    val interRows: List<NodeRow> = emptyList(),
)

/**
 * Écran principal, adaptatif : deux panneaux côte à côte sur écran large
 * (tablette, pliable ouvert, paysage), onglets glissables sur téléphone
 * portrait. `wide` est calculé depuis la WindowSizeClass par l'appelant.
 */
@Composable
fun MainScreen(
    state: MainUiState,
    wide: Boolean,
    onToggleCapture: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var mode by remember { mutableStateOf(ViewMode.GRAPH) }
    Column(
        modifier
            .fillMaxSize()
            .background(Visual.BG)
            // Edge-to-edge : on écarte les barres système (statut + navigation)
            // pour que la barre de contrôle et la légende restent atteignables.
            .windowInsetsPadding(WindowInsets.safeDrawing),
    ) {
        TopBar(
            vpn = state.vpn,
            mode = mode,
            onToggleMode = { mode = if (mode == ViewMode.GRAPH) ViewMode.LIST else ViewMode.GRAPH },
            onToggleCapture = onToggleCapture,
        )
        HorizontalDivider(color = Visual.PANEL_BORDER)
        var selected by remember { mutableStateOf<VizNode?>(null) }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            if (wide) {
                Row(Modifier.fillMaxSize()) {
                    Panel(
                        "Apps", state.appGraph, state.appRows, LayoutKind.CIRCLE, mode,
                        Modifier.weight(1f).fillMaxSize(), onSelect = { selected = it },
                    )
                    VerticalDivider(color = Visual.PANEL_BORDER)
                    Panel(
                        "Networks", state.interGraph, state.interRows, LayoutKind.NETWORKS, mode,
                        Modifier.weight(1f).fillMaxSize(), onSelect = { selected = it },
                    )
                }
            } else {
                PagerPanels(state, mode, onSelect = { selected = it })
            }
            selected?.let { node ->
                SelectionCard(node, Modifier.align(Alignment.BottomStart), onDismiss = { selected = null })
            }
        }
        HorizontalDivider(color = Visual.PANEL_BORDER)
        Legend(Modifier.fillMaxWidth())
    }
}

/** Un panneau, rendu en graphe ou en liste selon le mode. */
@Composable
private fun Panel(
    title: String,
    graph: GraphSnapshot,
    rows: List<NodeRow>,
    kind: LayoutKind,
    mode: ViewMode,
    modifier: Modifier = Modifier,
    onSelect: (VizNode?) -> Unit,
) {
    when (mode) {
        ViewMode.GRAPH -> GraphPanel(
            title, graph, kind, linkScale = 1f, protoFilter = null,
            modifier = modifier, onSelect = onSelect,
        )
        ViewMode.LIST -> NodePanel(title, rows, modifier)
    }
}

@Composable
private fun PagerPanels(state: MainUiState, mode: ViewMode, onSelect: (VizNode?) -> Unit) {
    var tab by remember { mutableIntStateOf(0) }
    Column(Modifier.fillMaxSize()) {
        PrimaryTabRow(selectedTabIndex = tab, containerColor = Visual.BG) {
            Tab(selected = tab == 0, onClick = { tab = 0 }, text = { Text("Apps") })
            Tab(selected = tab == 1, onClick = { tab = 1 }, text = { Text("Networks") })
        }
        if (tab == 0) {
            Panel(
                "Apps", state.appGraph, state.appRows, LayoutKind.CIRCLE, mode,
                Modifier.weight(1f).fillMaxSize(), onSelect,
            )
        } else {
            Panel(
                "Networks", state.interGraph, state.interRows, LayoutKind.NETWORKS, mode,
                Modifier.weight(1f).fillMaxSize(), onSelect,
            )
        }
    }
}

/** Carte de sélection (tap sur un nœud) : id, label, octets in/out. */
@Composable
private fun SelectionCard(node: VizNode, modifier: Modifier = Modifier, onDismiss: () -> Unit) {
    androidx.compose.material3.Surface(
        modifier = modifier.padding(12.dp),
        color = Visual.PANEL_BG,
        contentColor = Visual.FG,
        shape = androidx.compose.foundation.shape.RoundedCornerShape(8.dp),
        border = androidx.compose.foundation.BorderStroke(1.dp, Visual.PANEL_BORDER),
    ) {
        Column(Modifier.padding(12.dp)) {
            Text(node.id, style = MaterialTheme.typography.bodyMedium, color = Visual.FG)
            if (node.label != node.id) {
                Text(node.label, style = MaterialTheme.typography.bodySmall, color = Visual.MUTED)
            }
            Text(
                "out: ${formatBytes(node.bytesOut)}  ·  in: ${formatBytes(node.bytesIn)}  ·  ${node.proto}",
                style = MaterialTheme.typography.bodySmall,
                color = Visual.MUTED,
            )
            Text(
                "tap to dismiss",
                style = MaterialTheme.typography.labelSmall,
                color = Visual.ACCENT,
                modifier = Modifier
                    .semantics { contentDescription = "dismiss selection" }
                    .clickable { onDismiss() },
            )
        }
    }
}

@Composable
private fun TopBar(
    vpn: VpnState,
    mode: ViewMode,
    onToggleMode: () -> Unit,
    onToggleCapture: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            "Netman",
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            color = Visual.FG,
        )
        StatusChip(vpn, Modifier.weight(1f))
        // Bascule graphe ⇄ liste : le libellé annonce la vue de destination.
        androidx.compose.material3.TextButton(
            onClick = onToggleMode,
            modifier = Modifier.semantics {
                contentDescription = if (mode == ViewMode.GRAPH) "Switch to list view" else "Switch to graph view"
            },
        ) {
            Text(if (mode == ViewMode.GRAPH) "List" else "Graph", color = Visual.ACCENT)
        }
        val capturing = vpn is VpnState.Running || vpn is VpnState.Starting
        Button(
            onClick = onToggleCapture,
            colors = ButtonDefaults.buttonColors(
                containerColor = if (capturing) Visual.protoColor("ICMP") else Visual.ACCENT,
                contentColor = Visual.BG,
            ),
            modifier = Modifier.semantics { contentDescription = if (capturing) "Stop capture" else "Start capture" },
        ) {
            Text(if (capturing) "Stop" else "Start")
        }
    }
}

@Composable
private fun StatusChip(vpn: VpnState, modifier: Modifier = Modifier) {
    val (label, color) = when (vpn) {
        is VpnState.Idle -> "idle" to Visual.MUTED
        is VpnState.Starting -> "starting…" to Visual.protoColor("DNS")
        is VpnState.Running -> "live · ${vpn.frames} pkts" to Visual.protoColor("HTTP")
        is VpnState.Revoked -> "capture stopped" to Visual.protoColor("ICMP")
        is VpnState.Error -> "error: ${vpn.message}" to Visual.protoColor("ICMP")
    }
    Text(label, color = color, style = MaterialTheme.typography.bodySmall, modifier = modifier)
}
