package dev.jpcottin.netman.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import dev.jpcottin.netman.graph.Visual

/**
 * Thème sombre unique, calqué sur le client web (Netman est un moniteur : fond
 * sombre, accents par protocole). Pas de couleur dynamique — l'identité
 * visuelle prime.
 */
private val NetmanColors = darkColorScheme(
    primary = Visual.ACCENT,
    onPrimary = Visual.BG,
    background = Visual.BG,
    onBackground = Visual.FG,
    surface = Visual.PANEL_BG,
    onSurface = Visual.FG,
    surfaceVariant = Visual.PANEL_BG,
    onSurfaceVariant = Visual.MUTED,
    outline = Visual.PANEL_BORDER,
    error = Visual.protoColor("ICMP"),
)

@Composable
fun NetmanTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = NetmanColors, typography = Typography, content = content)
}
