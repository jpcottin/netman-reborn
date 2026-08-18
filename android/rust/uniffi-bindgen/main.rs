//! Générateur de bindings UniFFI (mode « library » : lit les métadonnées
//! embarquées dans le .so). Crate autonome pour ne pas recompiler la pile
//! bureau (pcap/axum) côté hôte. Invoqué par Gradle, voir app/build.gradle.kts.

fn main() {
    uniffi::uniffi_bindgen_main()
}
