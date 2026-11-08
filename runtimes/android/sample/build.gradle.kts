plugins {
    id("com.android.application")
}

android {
    namespace = "app.kineto.sample"
    compileSdk = 36

    defaultConfig {
        applicationId = "app.kineto.sample"
        minSdk = 24
        targetSdk = 36
        versionCode = 1
        versionName = "1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets {
        getByName("main") {
            // Documents the editor exported, read as any host would ship them.
            assets.srcDir(rootDir.resolve("../../engine/kineto_engine/tests/fixtures"))
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

dependencies {
    implementation(project(":KinetoRuntime"))
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test:core-ktx:1.7.0")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.lifecycle:lifecycle-common:2.9.4")
}
