plugins {
    id("com.android.application") version "8.11.1" apply false
    id("org.jetbrains.kotlin.android") version "2.2.21" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.2.21" apply false
    // Screenshots of every screen without a phone: ./gradlew recordPaparazziDebug
    id("app.cash.paparazzi") version "2.0.0-alpha05.1" apply false
}
