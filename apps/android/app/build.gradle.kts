plugins {
    id("com.android.application") version "8.9.1"
    kotlin("android") version "2.1.21"
    id("org.jetbrains.kotlin.plugin.compose") version "2.1.21"
}

fun String.asBuildConfigString() = "\"${replace("\\", "\\\\").replace("\"", "\\\"")}\""

val firebaseApiKey = providers.gradleProperty("jarvisFirebaseApiKey")
    .orElse(providers.environmentVariable("JARVIS_FIREBASE_API_KEY")).getOrElse("")
val firebaseProjectId = providers.gradleProperty("jarvisFirebaseProjectId")
    .orElse(providers.environmentVariable("JARVIS_FIREBASE_PROJECT_ID")).getOrElse("")
val firebaseAppId = providers.gradleProperty("jarvisFirebaseAppId")
    .orElse(providers.environmentVariable("JARVIS_FIREBASE_APP_ID")).getOrElse("")
val firebaseSenderId = providers.gradleProperty("jarvisFirebaseSenderId")
    .orElse(providers.environmentVariable("JARVIS_FIREBASE_SENDER_ID")).getOrElse("")

android {
    namespace = "local.jarvis.app"
    compileSdk = 35

    val releaseStoreFile = providers.gradleProperty("jarvisReleaseStoreFile").orNull
    val releaseStorePassword = providers.gradleProperty("jarvisReleaseStorePassword").orNull
    val releaseKeyAlias = providers.gradleProperty("jarvisReleaseKeyAlias").orNull
    val releaseKeyPassword = providers.gradleProperty("jarvisReleaseKeyPassword").orNull

    defaultConfig {
        applicationId = "local.jarvis.app"
        minSdk = 26
        targetSdk = 35
        versionCode = 101_300_001
        versionName = "1.13.1"
    }
    buildFeatures {
        compose = true
        buildConfig = true
    }
    defaultConfig {
        buildConfigField("String", "FIREBASE_API_KEY", firebaseApiKey.asBuildConfigString())
        buildConfigField("String", "FIREBASE_PROJECT_ID", firebaseProjectId.asBuildConfigString())
        buildConfigField("String", "FIREBASE_APP_ID", firebaseAppId.asBuildConfigString())
        buildConfigField("String", "FIREBASE_SENDER_ID", firebaseSenderId.asBuildConfigString())
    }
    signingConfigs {
        create("release") {
            if (!releaseStoreFile.isNullOrBlank()) storeFile = file(releaseStoreFile)
            storePassword = releaseStorePassword
            keyAlias = releaseKeyAlias
            keyPassword = releaseKeyPassword
        }
    }
    buildTypes {
        getByName("release") {
            signingConfig = signingConfigs.getByName("release")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }
}

val validateReleaseSigning = tasks.register("validateReleaseSigning") {
    doLast {
        val required: Map<String, String?> = mapOf(
            "jarvisReleaseStoreFile" to providers.gradleProperty("jarvisReleaseStoreFile").orNull,
            "jarvisReleaseStorePassword" to providers.gradleProperty("jarvisReleaseStorePassword").orNull,
            "jarvisReleaseKeyAlias" to providers.gradleProperty("jarvisReleaseKeyAlias").orNull,
            "jarvisReleaseKeyPassword" to providers.gradleProperty("jarvisReleaseKeyPassword").orNull,
        )
        val missing = required.filterValues { it.isNullOrBlank() }.keys
        check(missing.isEmpty()) {
            "APK release non firmabile: configura ${missing.joinToString()}."
        }
        check(file(required.getValue("jarvisReleaseStoreFile")!!).isFile) {
            "Keystore Android non trovato."
        }
    }
}

tasks.matching { it.name == "assembleRelease" || it.name == "bundleRelease" }.configureEach {
    dependsOn(validateReleaseSigning)
}

dependencies {
    implementation(project(":core-client"))
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    implementation("com.google.firebase:firebase-messaging:24.1.0")
}
