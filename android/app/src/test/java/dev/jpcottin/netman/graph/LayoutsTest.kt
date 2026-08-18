package dev.jpcottin.netman.graph

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.hypot

/** Port fidèle des règles de layout de `static/app.js` (grille de vérité). */
class LayoutsTest {

    @Test
    fun networkKey_ipv4_classful() {
        assertEquals("10.0.0.0/8", Layouts.networkKey("10.1.2.3"))
        assertEquals("127.0.0.0/8", Layouts.networkKey("127.0.0.1"))
        assertEquals("172.16.0.0/16", Layouts.networkKey("172.16.5.9"))
        assertEquals("192.168.0.0/24", Layouts.networkKey("192.168.0.42"))
        assertEquals("multicast & reserved", Layouts.networkKey("224.0.0.251"))
        assertEquals("multicast & reserved", Layouts.networkKey("240.0.0.1"))
    }

    @Test
    fun networkKey_ipv6_slash64() {
        assertEquals("v6 2001:db8:1:2::/64", Layouts.networkKey("2001:db8:1:2:3:4:5:6"))
        // Le :: est développé avant de prendre les 4 premiers groupes.
        assertEquals("v6 fe80:0:0:0::/64", Layouts.networkKey("fe80::1"))
    }

    @Test
    fun expandIpv6_fills_the_double_colon() {
        assertEquals(
            listOf("fe80", "0", "0", "0", "0", "0", "0", "1"),
            Layouts.expandIpv6("fe80::1"),
        )
        assertEquals(
            listOf("2001", "db8", "0", "0", "0", "0", "0", "1"),
            Layouts.expandIpv6("2001:db8::1"),
        )
        assertEquals(8, Layouts.expandIpv6("1:2:3:4:5:6:7:8").size)
    }

    @Test
    fun compareNodeIds_ipv4_is_numeric_not_lexical() {
        // "10" < "9" lexicalement, mais 9 < 10 numériquement.
        assertTrue(Layouts.compareNodeIds("10.0.0.9", "10.0.0.10") < 0)
        assertTrue(Layouts.compareNodeIds("10.0.0.2", "10.0.0.100") < 0)
    }

    @Test
    fun clusterRadius_zero_for_singleton() {
        assertEquals(0f, Layouts.clusterRadius(1), 0f)
        assertEquals(16f, Layouts.clusterRadius(2), 0.5f) // borne basse
        assertTrue(Layouts.clusterRadius(100) > 16f)
    }

    @Test
    fun circle_places_first_node_at_top_and_on_radius() {
        val result = Layouts.circle(listOf("c", "a", "b"), radius = 100f)
        // Trié : a,b,c → a est premier, à midi (x≈0, y≈-100).
        val a = result.positions.getValue("a")
        assertEquals(0f, a.x, 0.01f)
        assertEquals(-100f, a.y, 0.01f)
        // Tous les nœuds sur le cercle de rayon 100.
        result.positions.values.forEach {
            assertEquals(100f, hypot(it.x.toDouble(), it.y.toDouble()).toFloat(), 0.5f)
        }
        assertEquals(1, result.guides.size)
    }

    @Test
    fun circle_empty_has_no_guide() {
        assertTrue(Layouts.circle(emptyList()).guides.isEmpty())
    }

    @Test
    fun networks_singleton_group_has_no_guide_and_sits_on_ring() {
        // Un seul réseau, un seul hôte → rayon d'anneau et de cluster nuls.
        val r = Layouts.networks(listOf("10.0.0.1"))
        assertTrue(r.guides.isEmpty())
        val p = r.positions.getValue("10.0.0.1")
        assertEquals(0f, p.x, 0.01f)
        assertEquals(0f, p.y, 0.01f)
    }

    @Test
    fun networks_groups_by_classful_network() {
        val ids = listOf("10.0.0.1", "10.0.0.2", "192.168.1.5", "192.168.1.6", "192.168.1.7")
        val r = Layouts.networks(ids)
        // Deux réseaux (/8 et /24) → deux cercles guides.
        assertEquals(2, r.guides.size)
        assertEquals(ids.size, r.positions.size)
    }

    @Test
    fun networks_empty_is_empty() {
        val r = Layouts.networks(emptyList())
        assertTrue(r.positions.isEmpty())
        assertTrue(r.guides.isEmpty())
    }
}
