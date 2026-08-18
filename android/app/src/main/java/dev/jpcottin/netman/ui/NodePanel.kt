package dev.jpcottin.netman.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.background
import dev.jpcottin.netman.graph.Visual

/**
 * Panneau « liste » d'une vue (jalons 4-6, avant le rendu Canvas des graphes) :
 * un nœud par ligne, pastille de protocole, octets, degré.
 */
@Composable
fun NodePanel(
    title: String,
    rows: List<NodeRow>,
    modifier: Modifier = Modifier,
) {
    Column(modifier.semantics { contentDescription = "$title panel" }) {
        Text(
            text = "$title · ${rows.size}",
            style = MaterialTheme.typography.titleSmall,
            color = Visual.MUTED,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
        )
        if (rows.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text("No traffic yet", color = Visual.MUTED)
            }
        } else {
            LazyColumn(Modifier.fillMaxSize()) {
                items(rows, key = { it.id }) { row -> NodeRowItem(row) }
            }
        }
    }
}

@Composable
private fun NodeRowItem(row: NodeRow) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            Modifier
                .size(10.dp)
                .clip(CircleShape)
                .background(Visual.protoColor(row.proto)),
        )
        Column(Modifier.weight(1f)) {
            Text(
                text = row.label,
                color = Visual.FG,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodyMedium,
            )
            Text(
                text = "${row.proto} · ${row.edgeCount} link${if (row.edgeCount == 1) "" else "s"}",
                color = Visual.MUTED,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        Text(
            text = formatBytes(row.bytes),
            color = Visual.MUTED,
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

fun formatBytes(bytes: Long): String = when {
    bytes < 1024 -> "$bytes B"
    bytes < 1024 * 1024 -> String.format("%.1f KB", bytes / 1024.0)
    bytes < 1024L * 1024 * 1024 -> String.format("%.1f MB", bytes / (1024.0 * 1024))
    else -> String.format("%.2f GB", bytes / (1024.0 * 1024 * 1024))
}
