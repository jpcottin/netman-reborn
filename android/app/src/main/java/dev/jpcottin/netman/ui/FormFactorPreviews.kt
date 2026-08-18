package dev.jpcottin.netman.ui

import androidx.compose.ui.tooling.preview.Devices
import androidx.compose.ui.tooling.preview.Preview

/**
 * Aperçus multi-facteurs (cf. skill « adaptive », étape 1) : téléphone,
 * pliable, tablette, bureau. Appliqué aux composables clés pour vérifier
 * l'adaptativité et servir de base aux tests de capture d'écran.
 */
@Preview(name = "Phone", device = Devices.PHONE, showBackground = true)
@Preview(name = "Foldable", device = Devices.FOLDABLE, showBackground = true)
@Preview(name = "Tablet", device = Devices.TABLET, showBackground = true)
@Preview(name = "Desktop", device = Devices.DESKTOP, showBackground = true)
annotation class FormFactorPreviews
