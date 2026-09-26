plugins {
    id("com.android.application")
}

// The version is the workspace's, read from the root Cargo.toml, so the app and
// the desktop program can never disagree about which release this is.
val workspaceVersion: String = run {
    val cargo = rootDir.resolve("../../Cargo.toml").readText()
    Regex("""(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"""")
        .find(cargo)?.groupValues?.get(1)
        ?: error("no version under [workspace.package] in the root Cargo.toml")
}

// 0.6.10 becomes 6010: major × 100000 + minor × 1000 + patch. Always rising
// from one release to the next, as Play requires, while minor stays under 100
// and patch under 1000.
val workspaceVersionCode: Int = run {
    val (major, minor, patch) = workspaceVersion.split('.', '-', '+')
        .take(3)
        .map { part -> part.takeWhile(Char::isDigit).ifEmpty { "0" }.toInt() }
    require(minor < 100 && patch < 1000) { "$workspaceVersion does not fit the version code scheme" }
    major * 100_000 + minor * 1_000 + patch
}

// Release signing. The key and its passwords come from the environment or from
// Gradle properties (~/.gradle/gradle.properties, or -P on the command line),
// never from this repository:
//
//   EXV_KEYSTORE            exv.keystore            path to the .jks file
//   EXV_KEYSTORE_PASSWORD   exv.keystorePassword
//   EXV_KEY_ALIAS           exv.keyAlias
//   EXV_KEY_PASSWORD        exv.keyPassword
//
// With none of them set the release bundle is built unsigned. Play takes only a
// bundle signed with the app's upload key, so set them for anything going there.
// A keystore made by keytool on Java 9 or later is PKCS12, which has one
// password for the store and the key alike: give the same one twice.
fun secret(environment: String, property: String): String? =
    System.getenv(environment)?.takeIf { it.isNotBlank() }
        ?: (findProperty(property) as String?)?.takeIf { it.isNotBlank() }

val keystore = secret("EXV_KEYSTORE", "exv.keystore")

// The processors the program was built for, which are the only ones the app
// may claim. ML Kit carries its recogniser for four; an app that listed a
// processor the Rust library was not built for would install on it and close
// the moment it opened.
val builtAbis: List<String> = listOf("arm64-v8a", "x86_64").filter { abi ->
    layout.projectDirectory.file("src/main/jniLibs/$abi/libexcalibur_view_mobile.so").asFile.exists()
}

android {
    namespace = "com.excaliburct.view"
    compileSdk = 36
    buildToolsVersion = "36.1.0"

    defaultConfig {
        applicationId = "com.excaliburct.view"
        minSdk = 26
        targetSdk = 36
        versionCode = workspaceVersionCode
        versionName = workspaceVersion
        ndk {
            abiFilters += builtAbis
        }
    }

    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = secret("EXV_KEYSTORE_PASSWORD", "exv.keystorePassword")
                keyAlias = secret("EXV_KEY_ALIAS", "exv.keyAlias")
                keyPassword = secret("EXV_KEY_PASSWORD", "exv.keyPassword")
            }
        }
    }

    buildTypes {
        release {
            // The Java side is one small activity; the program is the Rust
            // library. Shrinking would save nothing worth having, and the
            // native glue finds GameActivity's methods by name, which R8 would
            // then need telling about one by one.
            isMinifyEnabled = false
            signingConfig = signingConfigs.findByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        jniLibs {
            // Installed as real files in the app's native library folder
            // rather than read in place from the APK. The tile helpers each
            // load their own copy of the PDF engine, and a copy needs a file
            // to be copied from. See MainActivity and hyperview's render.rs.
            useLegacyPackaging = true
            // build.sh has already stripped them and kept the symbols, so
            // Gradle needs no NDK and does not go looking for one.
            keepDebugSymbols += "**/*.so"
        }
    }
}

dependencies {
    // GameActivity is an AppCompatActivity but does not declare AppCompat.
    implementation("androidx.appcompat:appcompat:1.7.1")
    // The version android-activity 0.6's native glue is written against.
    implementation("androidx.games:games-activity:4.4.0")
    // AppCompat's own dependencies bring two generations of the Kotlin
    // standard library, whose old jdk7/jdk8 halves duplicate classes in the
    // new one. A project with Kotlin in it has the Kotlin plugin line them
    // up; this one is Java, so the Kotlin BOM does.
    implementation(platform("org.jetbrains.kotlin:kotlin-bom:1.8.22"))
    // Reading words off a scanned sheet (OCR), on the device. The bundled
    // model: nothing is downloaded later and nothing leaves the tablet.
    implementation("com.google.mlkit:text-recognition:16.0.1")
}

// An APK without the Rust library installs perfectly and then closes the moment
// it is opened, which is a confusing way to find out build.sh was not run.
val nativeLibraries = layout.projectDirectory.dir("src/main/jniLibs")
tasks.named("preBuild") {
    doFirst {
        val missing = listOf("arm64-v8a").filterNot { abi ->
            nativeLibraries.file("$abi/libexcalibur_view_mobile.so").asFile.exists() &&
                nativeLibraries.file("$abi/libpdfium.so").asFile.exists()
        }
        if (missing.isNotEmpty()) {
            throw GradleException(
                "No native libraries for ${missing.joinToString()} in app/src/main/jniLibs. " +
                    "Build with mobile/android/build.sh, which builds and copies them first."
            )
        }
    }
}
