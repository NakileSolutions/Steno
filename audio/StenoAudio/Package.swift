// swift-tools-version:5.10
//
// steno-audio : sidecar audio de Sténo (capture micro + process tap des apps de visio, VAD, ASR).
// Voir README.md pour la compilation et l'intégration comme `externalBin` Tauri.

import PackageDescription

let package = Package(
    name: "StenoAudio",
    platforms: [
        // Les process taps Core Audio exigent macOS 14.2 : le code concerné est annoté
        // `@available(macOS 14.2, *)` et le binaire refuse de démarrer en dessous.
        .macOS("14.2"),
    ],
    products: [
        .executable(name: "steno-audio", targets: ["StenoAudio"]),
    ],
    dependencies: [
        // FluidAudio (Apache 2.0) : Parakeet TDT v3, Silero VAD, diarisation hors ligne.
        // L'API évolue d'une version mineure à l'autre : Package.resolved fige la version
        // réellement utilisée. Passer à `.upToNextMinor(from:)` si une 0.19 casse l'API.
        .package(url: "https://github.com/FluidInference/FluidAudio.git", from: "0.18.4"),
    ],
    targets: [
        // Toute la logique (testable) vit dans la bibliothèque.
        .target(
            name: "StenoAudioKit",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio"),
            ],
            path: "Sources/StenoAudioKit"
        ),
        // Exécutable minimal : `main.swift` appelle `StenoAudioMain.run`.
        .executableTarget(
            name: "StenoAudio",
            dependencies: ["StenoAudioKit"],
            path: "Sources/StenoAudio",
            linkerSettings: [
                // Embarque Info.plist dans le binaire (section __TEXT,__info_plist) pour que
                // les descriptions d'usage (micro, audio système) soient présentes même quand
                // le sidecar est lancé seul (spikes). Dans Steno.app, c'est l'Info.plist de
                // l'app parente qui fait foi (voir README, « permissions TCC »).
                .unsafeFlags([
                    "-Xlinker", "-sectcreate",
                    "-Xlinker", "__TEXT",
                    "-Xlinker", "__info_plist",
                    "-Xlinker", "\(Context.packageDirectory)/Info.plist",
                ]),
            ]
        ),
        .testTarget(
            name: "StenoAudioTests",
            dependencies: ["StenoAudioKit"],
            path: "Tests/StenoAudioTests"
        ),
    ]
)
