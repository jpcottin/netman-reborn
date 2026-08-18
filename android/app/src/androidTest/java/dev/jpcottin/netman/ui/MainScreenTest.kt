package dev.jpcottin.netman.ui

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import dev.jpcottin.netman.theme.NetmanTheme
import dev.jpcottin.netman.vpn.VpnState
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class MainScreenTest {

    @get:Rule
    val rule = createComposeRule()

    @Test
    fun wide_layout_shows_both_panels_side_by_side() {
        rule.setContent {
            NetmanTheme {
                MainScreen(PreviewData.running, wide = true, onToggleCapture = {})
            }
        }
        rule.onNodeWithContentDescription("Apps panel").assertIsDisplayed()
        rule.onNodeWithContentDescription("Networks panel").assertIsDisplayed()
    }

    @Test
    fun narrow_layout_uses_tabs() {
        rule.setContent {
            NetmanTheme {
                MainScreen(PreviewData.running, wide = false, onToggleCapture = {})
            }
        }
        // Un seul panneau visible à la fois ; l'onglet Networks bascule.
        rule.onNodeWithContentDescription("Apps panel").assertIsDisplayed()
        rule.onNodeWithText("Networks").performClick()
        rule.onNodeWithContentDescription("Networks panel").assertIsDisplayed()
    }

    @Test
    fun start_button_shown_when_idle_and_triggers_callback() {
        var toggled = false
        rule.setContent {
            NetmanTheme {
                MainScreen(PreviewData.idle, wide = true, onToggleCapture = { toggled = true })
            }
        }
        rule.onNodeWithContentDescription("Start capture").assertIsDisplayed()
        rule.onNodeWithContentDescription("Start capture").performClick()
        assertTrue(toggled)
    }

    @Test
    fun stop_button_shown_when_running() {
        rule.setContent {
            NetmanTheme {
                MainScreen(
                    PreviewData.running.copy(vpn = VpnState.Running(frames = 42)),
                    wide = true,
                    onToggleCapture = {},
                )
            }
        }
        rule.onNodeWithContentDescription("Stop capture").assertIsDisplayed()
        rule.onNodeWithText("live · 42 pkts").assertIsDisplayed()
    }

    @Test
    fun panel_titles_show_node_counts() {
        rule.setContent {
            NetmanTheme {
                MainScreen(PreviewData.running, wide = true, onToggleCapture = {})
            }
        }
        // Titre = "<vue> · <nombre de nœuds>" dessiné en overlay du Canvas.
        rule.onNodeWithText("Apps · ${PreviewData.appGraph.nodes.size}").assertIsDisplayed()
        rule.onNodeWithText("Networks · ${PreviewData.interGraph.nodes.size}").assertIsDisplayed()
    }

    @Test
    fun graph_list_toggle_switches_representation() {
        rule.setContent {
            NetmanTheme {
                MainScreen(PreviewData.running, wide = true, onToggleCapture = {})
            }
        }
        // Départ en mode graphe : le bouton propose de passer en liste.
        rule.onNodeWithContentDescription("Switch to list view").assertIsDisplayed()
        rule.onNodeWithContentDescription("Switch to list view").performClick()
        // Après bascule : le bouton propose de revenir au graphe, et les lignes
        // de liste (label + protocole) apparaissent.
        rule.onNodeWithContentDescription("Switch to graph view").assertIsDisplayed()
        rule.onNodeWithText("Firefox").assertIsDisplayed()
    }

    @Test
    fun error_state_is_surfaced() {
        rule.setContent {
            NetmanTheme {
                MainScreen(
                    PreviewData.idle.copy(vpn = VpnState.Error("establish failed")),
                    wide = true,
                    onToggleCapture = {},
                )
            }
        }
        rule.onNodeWithText("error: establish failed").assertIsDisplayed()
    }
}
