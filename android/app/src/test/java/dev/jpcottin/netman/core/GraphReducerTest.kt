package dev.jpcottin.netman.core

import dev.jpcottin.netman.graph.GraphStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GraphReducerTest {

    private fun node(view: String, id: String, bytes: Long, proto: String) =
        """{"type":"upsert_node","view":"$view","id":"$id","label":"$id",""" +
            """"bytes":$bytes,"bytes_in":0,"bytes_out":$bytes,"packets":1,"proto":"$proto"}"""

    private fun edge(view: String, src: String, dst: String, proto: String) =
        """{"type":"upsert_edge","view":"$view","id":"$src|$dst","source":"$src",""" +
            """"target":"$dst","bytes":10,"packets":1,"proto":"$proto"}"""

    @Test
    fun routes_deltas_to_the_right_view() {
        val app = GraphStore()
        val inter = GraphStore()
        val batch = listOf(
            node("app", "app:10001", 100, "HTTPS"),
            node("inter", "10.0.0.1", 200, "DNS"),
        )
        GraphReducer.apply(app, inter, batch, emptySet(), paused = false)
        assertTrue(app.nodes.containsKey("app:10001"))
        assertTrue(inter.nodes.containsKey("10.0.0.1"))
        assertFalse(app.nodes.containsKey("10.0.0.1"))
    }

    @Test
    fun unknown_view_is_silently_dropped() {
        val app = GraphStore()
        val inter = GraphStore()
        // Vue "ether" (bureau) : inconnue du client Android.
        GraphReducer.apply(app, inter, listOf(node("ether", "aa:bb", 100, "IPv4")), emptySet(), false)
        assertTrue(app.nodes.isEmpty() && inter.nodes.isEmpty())
    }

    @Test
    fun config_updates_fade_and_reset_clears_views() {
        val app = GraphStore()
        val inter = GraphStore()
        GraphReducer.apply(app, inter, listOf(node("app", "app:1", 100, "HTTPS")), emptySet(), false)
        assertTrue(app.nodes.isNotEmpty())

        val r = GraphReducer.apply(
            app, inter,
            listOf("""{"type":"config","fade_secs":120}""", """{"type":"reset"}"""),
            emptySet(), false,
        )
        assertEquals(120L, r.fadeSecs)
        assertTrue(r.sawReset)
        assertTrue(app.nodes.isEmpty())
    }

    @Test
    fun paused_drops_graph_deltas_but_keeps_config_and_reset() {
        val app = GraphStore()
        val inter = GraphStore()
        app.upsertNode("app:1", "A", 1, 0, 1, 1, "HTTPS")

        val r = GraphReducer.apply(
            app, inter,
            listOf(
                node("app", "app:2", 100, "QUIC"), // ignoré (pause)
                """{"type":"config","fade_secs":30}""", // appliqué
            ),
            emptySet(), paused = true,
        )
        assertFalse(app.nodes.containsKey("app:2"))
        assertEquals(30L, r.fadeSecs)
    }

    @Test
    fun accumulates_seen_protocols() {
        val app = GraphStore()
        val inter = GraphStore()
        val r = GraphReducer.apply(
            app, inter,
            listOf(node("app", "app:1", 1, "HTTPS"), edge("app", "app:1", "1.2.3.4", "QUIC")),
            setOf("DNS"), false,
        )
        assertEquals(setOf("DNS", "HTTPS", "QUIC"), r.seenProtos)
    }

    @Test
    fun placeholder_proto_is_not_registered() {
        val app = GraphStore()
        val inter = GraphStore()
        val r = GraphReducer.apply(app, inter, listOf(node("app", "app:1", 1, "?")), emptySet(), false)
        assertTrue(r.seenProtos.isEmpty())
    }

    @Test
    fun absolute_counters_are_self_healing() {
        val app = GraphStore()
        val inter = GraphStore()
        // Deux upserts du même nœud : le second (valeur absolue) remplace.
        GraphReducer.apply(app, inter, listOf(node("inter", "10.0.0.1", 100, "DNS")), emptySet(), false)
        GraphReducer.apply(app, inter, listOf(node("inter", "10.0.0.1", 500, "DNS")), emptySet(), false)
        assertEquals(500, inter.nodes.getValue("10.0.0.1").bytes)
    }

    @Test
    fun remove_deltas_take_effect() {
        val app = GraphStore()
        val inter = GraphStore()
        GraphReducer.apply(app, inter, listOf(edge("inter", "10.0.0.1", "10.0.0.2", "DNS")), emptySet(), false)
        assertEquals(1, inter.edges.size)
        GraphReducer.apply(
            app, inter,
            listOf("""{"type":"remove_edge","view":"inter","id":"10.0.0.1|10.0.0.2"}"""),
            emptySet(), false,
        )
        assertTrue(inter.edges.isEmpty())
    }
}
