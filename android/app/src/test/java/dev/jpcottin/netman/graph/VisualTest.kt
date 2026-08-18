package dev.jpcottin.netman.graph

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Bornes et monotonie des tailles/épaisseurs (port de `static/app.js`). */
class VisualTest {

    @Test
    fun nodeSize_is_clamped_between_2_and_18() {
        assertEquals(2f, Visual.nodeSize(0), 0.001f)
        assertEquals(18f, Visual.nodeSize(Long.MAX_VALUE / 2), 0.001f)
        val mid = Visual.nodeSize(10_000)
        assertTrue(mid in 2f..18f)
    }

    @Test
    fun nodeSize_is_monotonic() {
        assertTrue(Visual.nodeSize(1000) < Visual.nodeSize(1_000_000))
    }

    @Test
    fun edgeWidth_is_capped_at_12() {
        assertEquals(12f, Visual.edgeWidth(rate = 1e12, scale = 3f), 0.001f)
    }

    @Test
    fun edgeWidth_scales_with_slider() {
        val small = Visual.edgeWidth(rate = 10_000.0, scale = 1f)
        val big = Visual.edgeWidth(rate = 10_000.0, scale = 3f)
        assertTrue(big > small)
    }

    @Test
    fun edgeWidth_floor_near_zero_rate() {
        assertEquals(0.4f, Visual.edgeWidth(rate = 0.0, scale = 1f), 0.001f)
    }

    @Test
    fun protoColor_known_and_fallback() {
        assertEquals(Visual.PROTO_COLORS.getValue("HTTPS"), Visual.protoColor("HTTPS"))
        assertEquals(Visual.OTHER_COLOR, Visual.protoColor("NOT_A_PROTO"))
    }

    @Test
    fun edgeColor_is_semi_transparent() {
        assertEquals(Visual.EDGE_ALPHA, Visual.edgeColor("HTTPS").alpha, 0.001f)
    }
}
