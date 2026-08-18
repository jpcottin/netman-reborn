package dev.jpcottin.netman

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.material3.windowsizeclass.ExperimentalMaterial3WindowSizeClassApi
import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.material3.windowsizeclass.calculateWindowSizeClass
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.jpcottin.netman.core.GraphRepository
import dev.jpcottin.netman.theme.NetmanTheme
import dev.jpcottin.netman.ui.MainScreen
import dev.jpcottin.netman.ui.MainUiState
import dev.jpcottin.netman.ui.toRows
import dev.jpcottin.netman.ui.toSnapshot
import dev.jpcottin.netman.vpn.VpnController
import dev.jpcottin.netman.vpn.VpnState

class MainActivity : ComponentActivity() {

    @OptIn(ExperimentalMaterial3WindowSizeClassApi::class)
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        maybeRequestNotifications()

        setContent {
            NetmanTheme {
                val windowSize = calculateWindowSizeClass(this)
                val wide = windowSize.widthSizeClass != WindowWidthSizeClass.Compact

                val vpn by VpnController.state.collectAsStateWithLifecycle()
                // frameTick est la seule invalidation : le relire déclenche la
                // reprojection des vues (jalons 4-6 : listes ; 7+ : Canvas).
                val tick by GraphRepository.frameTick.collectAsStateWithLifecycle()

                val state = rememberState(vpn, tick)

                // Consentement VPN puis démarrage du service.
                val consent = rememberLauncherForActivityResult(
                    ActivityResultContracts.StartActivityForResult(),
                ) { result ->
                    if (result.resultCode == RESULT_OK) VpnController.start(this)
                }

                Surface(Modifier.fillMaxSize()) {
                    MainScreen(
                        state = state,
                        wide = wide,
                        onToggleCapture = {
                            if (vpn is VpnState.Running || vpn is VpnState.Starting) {
                                VpnController.stop(this)
                            } else {
                                val intent = VpnController.consentIntent(this)
                                if (intent != null) consent.launch(intent) else VpnController.start(this)
                            }
                        },
                    )
                }
            }
        }
    }

    @Composable
    private fun rememberState(vpn: VpnState, @Suppress("UNUSED_PARAMETER") tick: Long): MainUiState =
        // tick force la recomposition ; les projections lisent l'état courant.
        MainUiState(
            vpn = vpn,
            appGraph = GraphRepository.appView.toSnapshot(),
            interGraph = GraphRepository.interView.toSnapshot(),
            appRows = GraphRepository.appView.toRows(),
            interRows = GraphRepository.interView.toRows(),
        )

    private fun maybeRequestNotifications() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS)
            != PackageManager.PERMISSION_GRANTED
        ) {
            val launcher = registerForActivityResult(
                ActivityResultContracts.RequestPermission(),
            ) { /* la capture fonctionne même si refusé */ }
            launcher.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }
}
