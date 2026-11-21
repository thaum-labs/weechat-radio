plugins {
    id("com.android.application") version "8.11.1" apply false
    id("org.jetbrains.kotlin.android") version "2.2.21" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.2.21" apply false
    // Screenshots of every screen without a phone: ./gradlew recordPaparazziDebug
    id("app.cash.paparazzi") version "1.3.5" apply false
}
