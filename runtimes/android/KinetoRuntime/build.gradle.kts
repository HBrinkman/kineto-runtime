plugins {
    id("com.android.library")
}

// Fixtures live where the other runtimes read them, addressed by repo-relative
// path in runtime_fixtures.json. They are copied, not moved, so Android runs the
// same bytes and the same expectations as the wasm parity runner.
val repoRoot = rootDir.resolve("../..").canonicalFile
val parityAssetsDir = layout.buildDirectory.dir("generated/parityAssets")
val copyParityAssets = tasks.register<Copy>("copyParityAssets") {
    into(parityAssetsDir)
    from(repoRoot) {
        include(
            "runtimes/parity/runtime_fixtures.json",
            "packages/kineto_runtime/example/assets/*.kvk",
            "engine/kineto_engine/tests/fixtures/*.kvk",
        )
    }
}

// libkineto_engine.so is generated, not committed (docs/BUILDING.md). Say
// so here: without this check the build succeeds and the app dies at
// System.loadLibrary, on the device, with an UnsatisfiedLinkError.
val checkKinetoEngineLibs = tasks.register("checkKinetoEngineLibs") {
    val libs = listOf("arm64-v8a", "x86_64").map {
        file("src/main/jniLibs/$it/libkineto_engine.so")
    }
    doLast {
        val missing = libs.filterNot { it.isFile }
        check(missing.isEmpty()) {
            "Missing ${missing.joinToString()}.\nRun scripts/build_runtime_android.sh from the repo root."
        }
    }
}

android {
    namespace = "app.kineto.runtime"
    compileSdk = 36

    defaultConfig {
        minSdk = 24
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets {
        getByName("main") {
            jniLibs.setSrcDirs(listOf("src/main/jniLibs"))
        }
        getByName("androidTest") {
            assets.srcDir(parityAssetsDir.get().asFile)
        }
    }
}

tasks.named("preBuild") {
    dependsOn(checkKinetoEngineLibs, copyParityAssets)
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

dependencies {
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test:core-ktx:1.7.0")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
}
