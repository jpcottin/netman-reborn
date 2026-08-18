package dev.jpcottin.netman.ui

import dev.jpcottin.netman.graph.GraphStore
import org.junit.Assert.assertEquals
import org.junit.Test

class PanelDataTest {

    @Test
    fun rows_are_sorted_by_bytes_descending() {
        val s = GraphStore()
        s.upsertNode("a", "A", 100, 0, 100, 1, "DNS")
        s.upsertNode("b", "B", 5000, 0, 5000, 1, "HTTPS")
        s.upsertNode("c", "C", 900, 0, 900, 1, "QUIC")
        val rows = s.toRows()
        assertEquals(listOf("b", "c", "a"), rows.map { it.id })
    }

    @Test
    fun edge_count_is_node_degree() {
        val s = GraphStore()
        s.upsertEdge("app:1|x", "app:1", "x", 10, 1, "DNS")
        s.upsertEdge("app:1|y", "app:1", "y", 10, 1, "DNS")
        val rows = s.toRows().associateBy { it.id }
        assertEquals(2, rows.getValue("app:1").edgeCount)
        assertEquals(1, rows.getValue("x").edgeCount)
    }

    @Test
    fun formatBytes_units() {
        assertEquals("512 B", formatBytes(512))
        assertEquals("1.0 KB", formatBytes(1024))
        assertEquals("1.0 MB", formatBytes(1024L * 1024))
        assertEquals("1.00 GB", formatBytes(1024L * 1024 * 1024))
    }
}
