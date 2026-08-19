package dev.jpcottin.netman.graph

import androidx.compose.ui.geometry.Rect

/**
 * Décombrement d'étiquettes : glouton par priorité. Les boîtes sont fournies
 * dans l'ordre de priorité (les plus gros nœuds d'abord) ; on garde une boîte
 * si elle ne recouvre aucune boîte déjà retenue. Fonction pure, testée sur la
 * JVM.
 */
object Declutter {
    /** @return un booléen par boîte : vrai = étiquette à dessiner. */
    fun keep(boxes: List<Rect>): BooleanArray {
        val result = BooleanArray(boxes.size)
        val placed = ArrayList<Rect>(boxes.size)
        for (i in boxes.indices) {
            val b = boxes[i]
            if (placed.none { it.overlaps(b) }) {
                result[i] = true
                placed.add(b)
            }
        }
        return result
    }
}
