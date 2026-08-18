//! Générateur de bindings Kotlin (mode « library » : lit les métadonnées
//! embarquées dans le .so). Invoqué par Gradle, voir android/app/build.gradle.kts.

fn main() {
    uniffi::uniffi_bindgen_main()
}
