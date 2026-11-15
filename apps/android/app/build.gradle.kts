import java.util.Base64

plugins {
    id("com.android.application")
}

// Without a fixed key every CI run signs with a fresh debug key, and Android
// refuses to update an app whose signature changed.
val keystoreBase64 = System.getenv("LAYON_KEYSTORE_BASE64").orEmpty()

android {
    namespace = "io.github.up9cloud.layon"
    compileSdk = 36

    defaultConfig {
        applicationId = "io.github.up9cloud.layon"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    signingConfigs {
        if (keystoreBase64.isNotEmpty()) {
            create("fixed") {
                val file = layout.buildDirectory.file("release.jks").get().asFile
                file.parentFile.mkdirs()
                file.writeBytes(Base64.getMimeDecoder().decode(keystoreBase64))
                storeFile = file
                storePassword = System.getenv("LAYON_KEYSTORE_PASSWORD")
                keyAlias = "layon"
                keyPassword = System.getenv("LAYON_KEYSTORE_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.findByName("fixed") ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // The CLI ships as liblayon.so so the installer unpacks it, executable,
    // into nativeLibraryDir; that is the only place an app may exec from.
    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }
}

dependencies {
    implementation("androidx.activity:activity-ktx:1.12.4")
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")
}
