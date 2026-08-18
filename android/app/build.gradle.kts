import org.gradle.api.tasks.Exec

plugins {
  alias(libs.plugins.android.application)
  alias(libs.plugins.compose.compiler)
  alias(libs.plugins.kotlin.serialization)
}

// --- Intégration Rust (netman-android) ---------------------------------------
// Deux tâches Exec, sans plugin (compatibilité AGP majeure garantie) :
//   cargoNdkBuild        : cross-compile le cdylib pour chaque ABI ;
//   generateUniffiBindings : génère le binding Kotlin (mode « library »).
// -PskipRust réutilise les sorties précédentes (itération UI rapide).

val rustDir = rootProject.file("rust/netman-android")
val rustManifest = rootProject.file("rust/netman-android/Cargo.toml")
val workspaceRoot = rootProject.file("..")
// Dossiers fixes (chemins connus) : l'API SourceSet d'AGP 9 refuse un
// Provider ; les dépendances de tâche sont recâblées explicitement plus bas.
val jniLibsDir = layout.buildDirectory.dir("rustJniLibs").get().asFile
val uniffiDir = layout.buildDirectory.dir("generated/uniffi/kotlin").get().asFile
val abis = listOf("arm64-v8a", "x86_64")
val ndkApi = 29
val skipRust = project.hasProperty("skipRust")

val cargoNdkBuild by tasks.registering(Exec::class) {
  group = "rust"
  description = "Cross-compile netman-android for ${abis.joinToString()}"
  workingDir = rustDir
  inputs.dir(rustDir.resolve("src"))
  inputs.file(rustManifest)
  inputs.files(workspaceRoot.resolve("src"))
  outputs.dir(jniLibsDir)
  val args = mutableListOf("cargo", "ndk")
  abis.forEach { args += listOf("-t", it) }
  args += listOf(
    "--platform", ndkApi.toString(),
    "-o", jniLibsDir.absolutePath,
    "build", "--release", "-p", "netman-android",
  )
  commandLine(args)
}

val generateUniffiBindings by tasks.registering(Exec::class) {
  group = "rust"
  description = "Generate Kotlin UniFFI bindings from the built library"
  dependsOn(cargoNdkBuild)
  workingDir = workspaceRoot
  val lib = jniLibsDir.resolve("arm64-v8a/libnetman_android.so")
  inputs.file(lib)
  outputs.dir(uniffiDir)
  commandLine(
    // Outil isolé (ne dépend que d'uniffi) : la génération ne recompile pas
    // la pile bureau, donc pas de dépendance à libpcap côté hôte.
    "cargo", "run", "--quiet", "-p", "netman-uniffi-bindgen", "--bin", "uniffi-bindgen",
    "--", "generate", "--library", lib.absolutePath,
    "--language", "kotlin", "--no-format",
    "--out-dir", uniffiDir.absolutePath,
  )
}

android {
  namespace = "dev.jpcottin.netman"
  compileSdk = 36
  defaultConfig {
    applicationId = "dev.jpcottin.netman"
    minSdk = 29
    targetSdk = 36
    versionCode = 1
    versionName = "1.0"
  }

  buildTypes {
    release {
      isMinifyEnabled = false
      proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
    }
  }
  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }
  buildFeatures {
    compose = true
    aidl = false
    buildConfig = false
    shaders = false
  }

  // Le .so Rust et le binding Kotlin généré rejoignent les sources. Les
  // chemins sont fixes ; les dépendances de tâche sont recâblées ci-dessous
  // (android.sourceset.disallowProvider=false autorise ces dossiers générés).
  sourceSets["main"].jniLibs.srcDir(jniLibsDir)
  sourceSets["main"].kotlin.srcDir(uniffiDir)

  packaging {
    resources { excludes += "/META-INF/{AL2.0,LGPL2.1}" }
    jniLibs { useLegacyPackaging = false }
  }
}

kotlin {
  jvmToolchain(17)
}

if (!skipRust) {
  // Câblage explicite des dépendances de tâche (non porté automatiquement
  // pour un srcDir fourni via chemin) : toute compilation Kotlin/merge JNI
  // attend la génération Rust.
  tasks.named("preBuild").configure { dependsOn(generateUniffiBindings) }
  tasks.matching {
    it.name.startsWith("merge") && it.name.contains("JniLibFolders")
  }.configureEach { dependsOn(cargoNdkBuild) }
  tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach {
    dependsOn(generateUniffiBindings)
  }
}

dependencies {
  val composeBom = platform(libs.androidx.compose.bom)
  implementation(composeBom)
  androidTestImplementation(composeBom)

  implementation(libs.androidx.core.ktx)
  implementation(libs.androidx.lifecycle.runtime.ktx)
  implementation(libs.androidx.activity.compose)
  implementation(libs.androidx.lifecycle.runtime.compose)
  implementation(libs.androidx.lifecycle.viewmodel.compose)

  implementation(libs.androidx.compose.ui)
  implementation(libs.androidx.compose.ui.tooling.preview)
  implementation(libs.androidx.compose.material3)
  implementation(libs.androidx.compose.material3.window.size)
  implementation(libs.androidx.compose.material3.adaptive)
  implementation(libs.androidx.compose.material3.adaptive.layout)
  debugImplementation(libs.androidx.compose.ui.tooling)
  androidTestImplementation(libs.androidx.compose.ui.test.junit4)
  debugImplementation(libs.androidx.compose.ui.test.manifest)

  // UniFFI : les bindings générés s'appuient sur JNA et les coroutines.
  implementation(libs.jna) { artifact { type = "aar" } }
  implementation(libs.kotlinx.serialization.json)

  testImplementation(libs.junit)
  testImplementation(libs.kotlinx.coroutines.test)

  androidTestImplementation(libs.androidx.test.core)
  androidTestImplementation(libs.androidx.test.ext.junit)
  androidTestImplementation(libs.androidx.test.runner)
  androidTestImplementation(libs.androidx.test.espresso.core)

  implementation(libs.androidx.navigation3.ui)
  implementation(libs.androidx.navigation3.runtime)
  implementation(libs.androidx.lifecycle.viewmodel.navigation3)
}
