import groovy.json.JsonSlurper

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

// The bridge's Android dependency graph, as Cargo resolves it: the crates
// whose JVM halves the app packages come from it.
val bridgePackages =
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
        packages.map { it as Map<*, *> }
    }.get()

val rustlsPlatformVerifierAndroid =
    run {
        val verifiers = bridgePackages.filter { it["name"] == "rustls-platform-verifier" }
        require(verifiers.size == 1) {
            "Android must contain one rustls-platform-verifier instance; found versions " +
                verifiers.map { it["version"] }
        }
        val verifierAndroid =
            bridgePackages
                .single { it["name"] == "rustls-platform-verifier-android" }
        Pair(
            file(verifierAndroid["manifest_path"] as String),
            verifierAndroid["version"] as String,
        )
    }

// coven's Android library, whose class takes the platform's network callbacks
// (fm.coven:coven-network): a Maven repository in coven-foundation's
// android/maven folder, which holds the one version this coven loads.
val covenNetwork =
    run {
        val foundations = bridgePackages.filter { it["name"] == "coven-foundation" }
        require(foundations.size == 1) {
            "Android must contain one coven-foundation instance; found versions " +
                foundations.map { it["version"] }
        }
        val repository =
            file(foundations.single()["manifest_path"] as String).parentFile.resolve("android/maven")
        val versions =
            repository.resolve("fm/coven/coven-network").listFiles { entry -> entry.isDirectory }
                ?.map { it.name }
                .orEmpty()
        require(versions.size == 1) {
            "coven-foundation's Maven repository must hold one coven-network version; found $versions"
        }
        Pair(repository, versions.single())
    }

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
        // The verifier's AAR comes only from the Cargo checkout it was built
        // with, bae's fork, never from another repository's build of the
        // same version.
        exclusiveContent {
            forRepository {
                maven {
                    url = uri(rustlsPlatformVerifierAndroid.first.parentFile.resolve("maven"))
                    metadataSources {
                        mavenPom()
                        artifact()
                    }
                }
            }
            filter { includeGroup("org.rustls") }
        }
        maven {
            url = uri(covenNetwork.first)
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
                "org.rustls",
                "rustls-platform-verifier",
            ).version(rustlsPlatformVerifierAndroid.second)
            library("coven-network", "fm.coven", "coven-network").version(covenNetwork.second)
        }
    }
}

rootProject.name = "bae"
include(":app", ":bridge", ":fastvalues", ":screensubscription")
