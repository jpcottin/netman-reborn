package dev.jpcottin.netman.ui

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.drawText
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.jpcottin.netman.graph.Declutter
import dev.jpcottin.netman.graph.GuideCircle
import dev.jpcottin.netman.graph.LayoutResult
import dev.jpcottin.netman.graph.Layouts
import dev.jpcottin.netman.graph.Point
import dev.jpcottin.netman.graph.Visual
import kotlin.math.max
import kotlin.math.min

/** Quelle projection de layout appliquer à cette vue. */
enum class LayoutKind { CIRCLE, NETWORKS }

private val EASE_OUT_CUBIC = CubicBezierEasing(0.215f, 0.61f, 0.355f, 1f)
private const val ANIM_MS = 600f
/** Seuil sigma : pas de label sous cette taille de nœud rendue (px écran). */
private const val LABEL_MIN_PX = 9f

/**
 * Un panneau graphe (Appman ou Interman) rendu sur un `Canvas` : placement
 * déterministe (cercle / réseaux-sur-anneau), animation ease-out-cubic,
 * pan/pinch, tap pour sélectionner. Port du rendu sigma.js du client web.
 */
@Composable
fun GraphPanel(
    title: String,
    snapshot: GraphSnapshot,
    kind: LayoutKind,
    linkScale: Float,
    protoFilter: String?,
    modifier: Modifier = Modifier,
    onSelect: (VizNode?) -> Unit = {},
) {
    val measurer = rememberTextMeasurer()
    val anim = remember { GraphAnimator() }

    // Transform utilisateur (au-dessus de l'ajustement automatique).
    var userScale by remember { mutableStateOf(1f) }
    var userPan by remember { mutableStateOf(Offset.Zero) }
    var selectedId by remember { mutableStateOf<String?>(null) }

    // Recalcule les cibles quand l'ensemble nœuds/arêtes change.
    anim.update(snapshot, kind)

    // Redessin piloté par l'animation : le compteur ne bouge que tant qu'un
    // nœud se déplace (les changements de données redessinent, eux, par
    // recomposition normale — nouveau `snapshot`). Au repos : zéro CPU.
    var redraw by remember { mutableStateOf(0) }
    LaunchedEffect(anim) {
        while (true) {
            withFrameNanos { now -> if (anim.tick(now)) redraw++ }
        }
    }

    Box(
        modifier
            .fillMaxSize()
            .background(Visual.BG)
            .semantics { contentDescription = "$title panel" }
            .pointerInput(Unit) {
                detectTransformGestures { _, pan, zoom, _ ->
                    userScale = (userScale * zoom).coerceIn(0.05f, 20f)
                    userPan += pan
                }
            }
            .pointerInput(snapshot.signature) {
                detectTapGestures { tap ->
                    val hit = anim.hitTest(tap, size.width, size.height, userScale, userPan)
                    selectedId = hit?.id
                    onSelect(hit)
                }
        },
    ) {
        Canvas(Modifier.fillMaxSize()) {
            redraw // lecture d'état : force le redraw à chaque frame d'animation
            drawGraph(anim, measurer, userScale, userPan, linkScale, protoFilter, selectedId)
        }
        androidx.compose.material3.Text(
            text = "$title · ${snapshot.nodes.size}",
            color = Visual.MUTED,
            style = androidx.compose.material3.MaterialTheme.typography.labelMedium,
            modifier = Modifier.padding(10.dp),
        )
    }
}

/** État animé d'un panneau : positions courantes glissant vers les cibles. */
class GraphAnimator {
    private var currentSig = 0
    private var kind = LayoutKind.CIRCLE
    var snapshot = GraphSnapshot(emptyList(), emptyList())
        private set

    // Positions (repère graphe) : from → target, interpolées par `progress`.
    private val fromPos = HashMap<String, Point>()
    private val targetPos = HashMap<String, Point>()
    private val curPos = HashMap<String, Point>()
    private var guides: List<GuideCircle> = emptyList()

    private var animStart = 0L
    private var animating = false
    // Bornes du graphe (pour l'ajustement automatique), en repère graphe.
    var bounds = Bounds(-120f, -120f, 120f, 120f)
        private set

    fun update(snapshot: GraphSnapshot, kind: LayoutKind) {
        if (snapshot.signature == currentSig && kind == this.kind) {
            this.snapshot = snapshot // maj des compteurs sans relayout
            return
        }
        currentSig = snapshot.signature
        this.kind = kind
        this.snapshot = snapshot

        val ids = snapshot.nodes.map { it.id }
        val layout: LayoutResult =
            if (kind == LayoutKind.CIRCLE) Layouts.circle(ids) else Layouts.networks(ids)
        guides = layout.guides

        fromPos.clear()
        targetPos.clear()
        for ((id, p) in layout.positions) {
            targetPos[id] = p
            // Nœud existant : anime depuis sa position courante ; nouveau nœud :
            // apparaît directement à sa cible (comportement `placed` de app.js).
            fromPos[id] = curPos[id] ?: p
        }
        // Purge des nœuds disparus.
        curPos.keys.retainAll(targetPos.keys)
        recomputeBounds()
        animStart = 0L
        animating = true
    }

    /** @return true si des positions ont bougé (déclenche un redraw). */
    fun tick(nowNanos: Long): Boolean {
        if (!animating) return false
        if (animStart == 0L) animStart = nowNanos
        val elapsedMs = (nowNanos - animStart) / 1_000_000f
        val t = (elapsedMs / ANIM_MS).coerceIn(0f, 1f)
        val e = EASE_OUT_CUBIC.transform(t)
        for ((id, target) in targetPos) {
            val from = fromPos[id] ?: target
            curPos[id] = Point(from.x + (target.x - from.x) * e, from.y + (target.y - from.y) * e)
        }
        if (t >= 1f) animating = false
        return true
    }

    fun positionsAndGuides(): Pair<Map<String, Point>, List<GuideCircle>> = curPos to guides

    private fun recomputeBounds() {
        if (targetPos.isEmpty()) {
            bounds = Bounds(-120f, -120f, 120f, 120f)
            return
        }
        var minX = Float.MAX_VALUE; var minY = Float.MAX_VALUE
        var maxX = -Float.MAX_VALUE; var maxY = -Float.MAX_VALUE
        for (p in targetPos.values) {
            minX = min(minX, p.x); minY = min(minY, p.y)
            maxX = max(maxX, p.x); maxY = max(maxY, p.y)
        }
        for (g in guides) {
            minX = min(minX, g.cx - g.r); minY = min(minY, g.cy - g.r)
            maxX = max(maxX, g.cx + g.r); maxY = max(maxY, g.cy + g.r)
        }
        // Marge pour la taille des nœuds/labels.
        bounds = Bounds(minX - 30f, minY - 30f, maxX + 30f, maxY + 30f)
    }

    /** Échelle+décalage d'ajustement automatique pour tenir dans le canvas. */
    fun fit(w: Float, h: Float): Pair<Float, Offset> {
        val bw = max(1f, bounds.maxX - bounds.minX)
        val bh = max(1f, bounds.maxY - bounds.minY)
        val scale = min(w / bw, h / bh)
        val cx = (bounds.minX + bounds.maxX) / 2f
        val cy = (bounds.minY + bounds.maxY) / 2f
        // Centre du graphe → centre du canvas.
        val offset = Offset(w / 2f - cx * scale, h / 2f - cy * scale)
        return scale to offset
    }

    /** Nœud sous le point d'écran (ou null). */
    fun hitTest(tap: Offset, w: Int, h: Int, userScale: Float, userPan: Offset): VizNode? {
        val (base, baseOff) = fit(w.toFloat(), h.toFloat())
        val scale = base * userScale
        val off = baseOff + userPan
        var best: VizNode? = null
        var bestDist = Float.MAX_VALUE
        val byId = snapshot.nodes.associateBy { it.id }
        for ((id, p) in curPos) {
            val sx = p.x * scale + off.x
            val sy = p.y * scale + off.y
            val node = byId[id] ?: continue
            // Rayon de sélection : taille du nœud à l'écran, min 20 px (cible
            // tactile confortable même pour les petits nœuds).
            val r = max(Visual.nodeSize(node.bytes), 20f)
            val d = (tap.x - sx) * (tap.x - sx) + (tap.y - sy) * (tap.y - sy)
            if (d < r * r && d < bestDist) { bestDist = d; best = node }
        }
        return best
    }

    data class Bounds(val minX: Float, val minY: Float, val maxX: Float, val maxY: Float)
}

private fun DrawScope.drawGraph(
    anim: GraphAnimator,
    measurer: TextMeasurer,
    userScale: Float,
    userPan: Offset,
    linkScale: Float,
    protoFilter: String?,
    selectedId: String?,
) {
    val (positions, guides) = anim.positionsAndGuides()
    if (positions.isEmpty()) return
    val (base, baseOff) = anim.fit(size.width, size.height)
    val scale = base * userScale
    val off = baseOff + userPan
    fun sx(x: Float) = x * scale + off.x
    fun sy(y: Float) = y * scale + off.y

    // Cercles guides (pointillés), sous tout le reste.
    val dash = PathEffect.dashPathEffect(floatArrayOf(4f, 5f))
    for (g in guides) {
        val rr = g.r * scale
        if (rr < 3f) continue
        drawCircle(
            color = Visual.GUIDE,
            radius = rr,
            center = Offset(sx(g.cx), sy(g.cy)),
            style = Stroke(width = 1f, pathEffect = dash),
        )
    }

    val byId = anim.snapshot.nodes.associateBy { it.id }

    // Arêtes : couleur = protocole, épaisseur ∝ débit lissé (EWMA, octets/s).
    for (e in anim.snapshot.edges) {
        val a = positions[e.source] ?: continue
        val b = positions[e.target] ?: continue
        val dimmed = protoFilter != null && e.proto != protoFilter
        if (dimmed) continue
        val width = Visual.edgeWidth(e.rate, linkScale)
        drawLine(
            color = Visual.edgeColor(e.proto),
            start = Offset(sx(a.x), sy(a.y)),
            end = Offset(sx(b.x), sy(b.y)),
            strokeWidth = width,
        )
    }

    // Nœuds : taille ∝ log(octets), couleur = protocole dominant. On collecte
    // au passage les candidats-étiquettes pour le décombrement.
    data class LabelBox(val label: String, val center: Offset, val r: Float, val bytes: Long)
    val labelCandidates = ArrayList<LabelBox>()
    for ((id, p) in positions) {
        val node = byId[id] ?: continue
        val dimmed = protoFilter != null && node.proto != protoFilter
        val r = max(Visual.nodeSize(node.bytes), 2f)
        val color = if (dimmed) Visual.DIMMED_COLOR else Visual.protoColor(node.proto)
        val center = Offset(sx(p.x), sy(p.y))
        drawCircle(color = color, radius = r, center = center)
        if (id == selectedId) {
            drawCircle(
                color = Visual.ACCENT,
                radius = r + 4f,
                center = center,
                style = Stroke(width = 2f),
            )
        }
        if (!dimmed && r >= LABEL_MIN_PX) {
            labelCandidates.add(LabelBox(node.label, center, r, node.bytes))
        }
    }

    // Étiquettes décombrées : les plus gros nœuds d'abord ; une étiquette
    // n'est dessinée que si sa boîte ne recouvre aucune déjà posée (le nœud
    // sélectionné est prioritaire pour rester toujours lisible).
    val ordered = labelCandidates.sortedWith(
        compareByDescending<LabelBox> { it.label == selectedLabel(byId, selectedId) }
            .thenByDescending { it.bytes },
    )
    val layouts = ordered.map {
        measurer.measure(it.label, TextStyle(color = Visual.FG, fontSize = 11.sp))
    }
    val boxes = ordered.mapIndexed { i, c ->
        val topLeft = Offset(c.center.x + c.r + 3f, c.center.y - layouts[i].size.height / 2f)
        Rect(topLeft, Size(layouts[i].size.width.toFloat(), layouts[i].size.height.toFloat()))
    }
    val keep = Declutter.keep(boxes)
    for (i in ordered.indices) {
        if (!keep[i]) continue
        drawText(textLayoutResult = layouts[i], topLeft = boxes[i].topLeft)
    }
}

/** Label du nœud sélectionné (pour le prioriser au décombrement), ou null. */
private fun selectedLabel(byId: Map<String, VizNode>, selectedId: String?): String? =
    selectedId?.let { byId[it]?.label }
