package dev.jpcottin.netman.graph

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GraphStoreTest {

    @Test
    fun upsert_node_is_idempotent_on_id() {
        val s = GraphStore()
        s.upsertNode("a", "A", 100, 40, 60, 2, "HTTPS")
        s.upsertNode("a", "A2", 200, 80, 120, 4, "QUIC")
        assertEquals(1, s.nodes.size)
        val n = s.nodes.getValue("a")
        assertEquals("A2", n.label)
        assertEquals(200, n.bytes)
        assertEquals("QUIC", n.proto)
    }

    @Test
    fun edge_before_endpoints_creates_stub_nodes() {
        val s = GraphStore()
        s.upsertEdge("a|b", "a", "b", 10, 1, "DNS")
        // Extrémités créées à la volée (id == label, souche).
        assertEquals(2, s.nodes.size)
        assertTrue(s.nodes.containsKey("a"))
        assertEquals("a", s.nodes.getValue("a").label)
        assertEquals(1, s.edges.size)
    }

    @Test
    fun remove_node_and_edge() {
        val s = GraphStore()
        s.upsertEdge("a|b", "a", "b", 10, 1, "DNS")
        s.removeEdge("a|b")
        assertTrue(s.edges.isEmpty())
        s.removeNode("a")
        assertFalse(s.nodes.containsKey("a"))
    }

    @Test
    fun edge_rate_ewma_seeds_at_zero_then_tracks_throughput() {
        val s = GraphStore()
        // Premier upsert : amorçage, débit nul.
        s.upsertEdge("a|b", "a", "b", 0, 0, "HTTPS", now = 0.0)
        assertEquals(0.0, s.edges.getValue("a|b").rate, 1e-9)

        // 1000 octets/s pendant plusieurs pas : l'EWMA converge vers ~1000.
        var bytes = 0L
        for (i in 1..30) {
            bytes += 1000
            s.upsertEdge("a|b", "a", "b", bytes, i.toLong(), "HTTPS", now = i.toDouble())
        }
        assertEquals(1000.0, s.edges.getValue("a|b").rate, 60.0)
    }

    @Test
    fun edge_rate_decays_when_silent() {
        val s = GraphStore()
        s.upsertEdge("a|b", "a", "b", 0, 0, "DNS", now = 0.0)
        // Amène le débit au-dessus de 1 pour que la décroissance s'applique.
        s.upsertEdge("a|b", "a", "b", 10_000, 1, "DNS", now = 1.0)
        val before = s.edges.getValue("a|b").rate
        assertTrue(before > 1.0)
        // Silence > 2 s : le débit décroît.
        s.decayRates(now = 4.0)
        assertTrue(s.edges.getValue("a|b").rate < before)
    }

    @Test
    fun clear_empties_everything() {
        val s = GraphStore()
        s.upsertEdge("a|b", "a", "b", 10, 1, "DNS")
        s.clear()
        assertTrue(s.nodes.isEmpty() && s.edges.isEmpty())
    }
}
