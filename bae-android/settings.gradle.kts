import groovy.json.JsonSlurper

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

val rustlsPlatformVerifierAndroid =
    providers.exec {
        workingDir = rootDir.parentFile
        commandLine(
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            "aarch64-linux-android",
            "--manifest-path",
            "bae-bridge/Cargo.toml",
        )
    }.standardOutput.asText.map { metadata ->
        val packages =
            (JsonSlurper().parseText(metadata) as Map<*, *>)["packages"] as List<*>
        val verifier =
            packages
                .map { it as Map<*, *> }
                .single { it["name"] == "rustls-platform-verifier-android" }
        Pair(
            file(verifier["manifest_path"] as String),
            verifier["version"] as String,
        )
    }.get()

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
        maven {
            url = uri(rustlsPlatformVerifierAndroid.first.parentFile.resolve("maven"))
            metadataSources {
                mavenPom()
                artifact()
            }
        }
    }
    versionCatalogs {
        create("nativeDeps") {
            library(
                "rustls-platform-verifier",
                "rustls",
                "rustls-platform-verifier",
            ).version(rustlsPlatformVerifierAndroid.second)
        }
    }
}

rootProject.name = "bae"
include(":app")
