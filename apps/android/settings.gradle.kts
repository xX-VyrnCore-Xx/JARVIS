// Client Android di JARVIS (RF-10, ADR-0018).
// - core-client: logica di rete in Kotlin puro, verificabile senza Android SDK.
// - app: interfaccia Jetpack Compose; si compila dove è presente l'Android SDK (CI). Con -Pjarvis.coreOnly=true
//   si configura solo core-client, per esempio su un computer senza SDK.
pluginManagement {
    repositories {
        mavenCentral()
        google()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositories {
        mavenCentral()
        google()
    }
}
rootProject.name = "jarvis-android"
include(":core-client")
if (providers.gradleProperty("jarvis.coreOnly").orNull != "true") {
    include(":app")
}
