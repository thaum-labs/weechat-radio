plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// The app version follows the wcr workspace version (root Cargo.toml),
// so the phone and the desktop app always say the same number.
val wcrVersion: String = rootProject.file("../Cargo.toml").readLines()
    .first { it.startsWith("version = ") }
    .substringAfter('"').substringBefore('"')

android {
    namespace = "com.weechatradio.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.weechatradio.app"
        minSdk = 26
        targetSdk = 35
        // Bumped by CI from the run number; 1 for local builds.
        versionCode = (System.getenv("ANDROID_VERSION_CODE") ?: "1").toInt()
        versionName = wcrVersion
        // Only 64-bit ARM phones for now (the Moto G17 and nearly every phone since 2019).
        ndk { abiFilters += "arm64-v8a" }
    }

    // The release key comes from four environment variables (GitHub secrets in CI).
    // Without them the release build is left unsigned; debug builds always work.
    val keystorePath = System.getenv("ANDROID_KEYSTORE_PATH")
    signingConfigs {
        if (keystorePath != null) {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (keystorePath != null) signingConfig = signingConfigs.getByName("release")
        }
    }

    // Kotlin bindings for the wcr core, written by build-native.sh.
    sourceSets["main"].java.srcDir("build/generated/uniffi")

    packaging {
        // modem73 ships as libmodem73.so and is run as a program, so it must
        // be unpacked on install (Android only runs programs from there).
        jniLibs.useLegacyPackaging = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        compose = true
        buildConfig = true
    }

    // Screenshots of every screen on the computer (Robolectric + Roborazzi):
    // ./gradlew testDebugUnitTest writes them to app/build/screens/.
    testOptions.unitTests {
        isIncludeAndroidResources = true
        all { it.systemProperty("roborazzi.test.record", "true") }
    }
}

kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2025.06.00")
    implementation(composeBom)
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    debugImplementation("androidx.compose.ui:ui-tooling")
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.core:core-ktx:1.16.0")
    // UniFFI's Kotlin bindings call the Rust core through JNA.
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.15.1")
    testImplementation("io.github.takahirom.roborazzi:roborazzi:1.46.1")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-compose:1.46.1")
    testImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
