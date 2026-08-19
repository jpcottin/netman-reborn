package dev.jpcottin.netman.ui

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import dev.jpcottin.netman.theme.NetmanTheme
import dev.jpcottin.netman.vpn.VpnState

/** Données d'exemple pour les aperçus et tests (aucune dépendance runtime). */
object PreviewData {
    private fun n(id: String, label: String, bytes: Long, proto: String) =
        VizNode(id, label, bytes, bytes / 2, bytes / 2, proto)

    val appGraph = GraphSnapshot(
        nodes = listOf(
            n("app:10001", "Firefox", 2_400_000, "HTTPS"),
            n("app:10002", "System (1051)", 48_000, "DNS"),
            n("app:10003", "Maps", 890_000, "QUIC"),
            n("app:-1", "Unknown", 12_000, "ICMP"),
            n("93.184.216.34", "example.com", 2_400_000, "HTTPS"),
            n("10.0.0.53", "router.lan", 48_000, "DNS"),
            n("142.250.1.1", "142.250.1.1", 890_000, "QUIC"),
        ),
        edges = listOf(
            VizEdge("app:10001|93.184.216.34", "app:10001", "93.184.216.34", 2_400_000, 120_000.0, "HTTPS"),
            VizEdge("app:10002|10.0.0.53", "app:10002", "10.0.0.53", 48_000, 800.0, "DNS"),
            VizEdge("app:10003|142.250.1.1", "app:10003", "142.250.1.1", 890_000, 40_000.0, "QUIC"),
        ),
    )
    val interGraph = GraphSnapshot(
        nodes = listOf(
            n("93.184.216.34", "example.com", 2_400_000, "HTTPS"),
            n("10.0.0.53", "router.lan", 48_000, "DNS"),
            n("10.0.0.1", "10.0.0.1", 200_000, "TCP"),
            n("2606:2800:220::", "cdn.example", 890_000, "QUIC"),
            n("224.0.0.251", "224.0.0.251", 6_000, "mDNS"),
        ),
        edges = listOf(
            VizEdge("10.0.0.1|93.184.216.34", "10.0.0.1", "93.184.216.34", 2_400_000, 120_000.0, "HTTPS"),
            VizEdge("10.0.0.1|10.0.0.53", "10.0.0.1", "10.0.0.53", 48_000, 800.0, "DNS"),
        ),
    )
    private fun rows(g: GraphSnapshot): List<NodeRow> {
        val degree = HashMap<String, Int>()
        for (e in g.edges) {
            degree[e.source] = (degree[e.source] ?: 0) + 1
            degree[e.target] = (degree[e.target] ?: 0) + 1
        }
        return g.nodes.map { NodeRow(it.id, it.label, it.bytes, it.proto, degree[it.id] ?: 0) }
            .sortedByDescending { it.bytes }
    }

    val running = MainUiState(
        vpn = VpnState.Running(frames = 1234, bytes = 3_500_000),
        appGraph = appGraph,
        interGraph = interGraph,
        appRows = rows(appGraph),
        interRows = rows(interGraph),
    )
    val idle = MainUiState(vpn = VpnState.Idle)
}

@FormFactorPreviews
@Composable
private fun MainScreenWidePreview() {
    NetmanTheme {
        MainScreen(PreviewData.running, wide = true, onToggleCapture = {}, modifier = Modifier.fillMaxSize())
    }
}

@Preview(name = "Phone portrait", widthDp = 360, heightDp = 740, showBackground = true)
@Composable
private fun MainScreenPhonePreview() {
    NetmanTheme {
        MainScreen(PreviewData.running, wide = false, onToggleCapture = {}, modifier = Modifier.fillMaxSize())
    }
}

@Preview(name = "Idle empty", widthDp = 360, heightDp = 740, showBackground = true)
@Composable
private fun MainScreenIdlePreview() {
    NetmanTheme {
        MainScreen(PreviewData.idle, wide = false, onToggleCapture = {}, modifier = Modifier.fillMaxSize())
    }
}

@Preview(name = "Apps graph", showBackground = true, widthDp = 400, heightDp = 400)
@Composable
private fun AppsGraphPreview() {
    NetmanTheme {
        GraphPanel(
            "Apps", PreviewData.appGraph, LayoutKind.CIRCLE,
            linkScale = 1f, protoFilter = null, modifier = Modifier.fillMaxSize(),
        )
    }
}

@Preview(name = "Networks graph", showBackground = true, widthDp = 400, heightDp = 400)
@Composable
private fun NetworksGraphPreview() {
    NetmanTheme {
        GraphPanel(
            "Networks", PreviewData.interGraph, LayoutKind.NETWORKS,
            linkScale = 1f, protoFilter = null, modifier = Modifier.fillMaxSize(),
        )
    }
}

@Preview(name = "Legend", showBackground = true, widthDp = 360)
@Composable
private fun LegendPreview() {
    NetmanTheme { Legend() }
}
