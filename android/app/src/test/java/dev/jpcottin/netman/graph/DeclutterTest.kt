package dev.jpcottin.netman.graph

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class DeclutterTest {

    private fun box(x: Float, y: Float, w: Float = 40f, h: Float = 12f) =
        Rect(Offset(x, y), Size(w, h))

    @Test
    fun non_overlapping_boxes_are_all_kept() {
        val boxes = listOf(box(0f, 0f), box(0f, 20f), box(0f, 40f))
        assertArrayEquals(booleanArrayOf(true, true, true), Declutter.keep(boxes))
    }

    @Test
    fun overlapping_box_is_dropped_keeping_the_earlier_priority() {
        // Deuxième boîte recouvre la première (priorité = ordre d'entrée).
        val boxes = listOf(box(0f, 0f), box(5f, 3f), box(0f, 40f))
        assertArrayEquals(booleanArrayOf(true, false, true), Declutter.keep(boxes))
    }

    @Test
    fun decluttering_is_measured_against_kept_boxes_only() {
        // b1 gardée ; b2 recouvre b1 → tombe ; b3 loin de la seule retenue
        // (b1) → gardée. Le test se fait contre les RETENUES, pas les tombées.
        val boxes = listOf(box(0f, 0f), box(5f, 3f), box(100f, 0f))
        assertArrayEquals(booleanArrayOf(true, false, true), Declutter.keep(boxes))
    }

    @Test
    fun empty_input_returns_empty() {
        assertArrayEquals(booleanArrayOf(), Declutter.keep(emptyList()))
    }
}
