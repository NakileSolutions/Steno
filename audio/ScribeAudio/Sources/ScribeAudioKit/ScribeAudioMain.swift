// Point d'entrée : mode protocole (par défaut) ou modes spike.
//
// Mode protocole :
//   - un thread dédié lit stdin ligne par ligne ; les commandes sont traitées dans l'ordre
//     par une seule boucle asynchrone ;
//   - stdout est écrit par une file unique (Output), une ligne JSON par message ;
//   - journaux sur stderr uniquement ;
//   - EOF sur stdin, `shutdown`, SIGTERM ou SIGINT → arrêt propre.

import Foundation

public enum ScribeAudioMain {
    static let usage = """
    scribe-audio \(SidecarInfo.version) — sidecar audio de Scribe

    Usage :
      scribe-audio                                   mode protocole (JSON lines sur stdin/stdout)
      scribe-audio --spike-record <secondes> <dossier> [--apps teams,slack]
                                                     enregistre me.wav et them.wav (spike S1)
      scribe-audio --spike-transcribe <fichier.wav>  transcrit un fichier, temps par phrase (spike S2)
      scribe-audio --spike-compare-asr <f.wav>…     compare Parakeet v3 et Canary (français imposé)
      scribe-audio --list-audio-processes [--apps teams,slack]
                                                     liste les processus audio vus par Core Audio
                                                     et l'app reconnue pour chacun
      scribe-audio --list-devices                    liste les micros et sorties avec leur UID
                                                     (valeurs de input_device / set_input / play)
      scribe-audio --prefetch-models                 télécharge et prépare les modèles (make install)
      scribe-audio --version

    Apps connues de --apps : \(WatchedApp.catalog.map(\.id).joined(separator: ", ")) (par défaut : teams).
    En mode protocole, la liste des apps vient du cœur (commande watch).

    Variables d'environnement :
      SCRIBE_TAP_MODE=global   tap sur tout le son du Mac (hors Scribe) au lieu des apps de visio
      SCRIBE_VAD=energy        VAD par énergie au lieu de Silero
      SCRIBE_MODELS_DIR=<dir>  dossier des modèles (sinon cache de FluidAudio)
      SCRIBE_AUDIO_DEBUG=1     journaux détaillés sur stderr
    """

    public static func run(arguments: [String]) -> Never {
        // Si le cœur disparaît, write(2) renvoie EPIPE au lieu de tuer le processus.
        signal(SIGPIPE, SIG_IGN)

        guard #available(macOS 14.2, *) else {
            fputs("scribe-audio nécessite macOS 14.2 ou plus récent (process taps Core Audio).\n", stderr)
            exit(2)
        }

        let args = Array(arguments.dropFirst())
        switch args.first ?? "" {
        case "":
            runProtocol()

        case "--version":
            print(SidecarInfo.version)
            exit(0)

        case "--help", "-h":
            print(usage)
            exit(0)

        case "--spike-record":
            guard args.count >= 3, let seconds = Double(args[1]), seconds > 0 else {
                fputs(usage + "\n", stderr)
                exit(64)
            }
            let apps = appsOption(Array(args.dropFirst(3)))
            let dir = URL(fileURLWithPath: (args[2] as NSString).expandingTildeInPath, isDirectory: true)
            Task {
                let code = await Spikes.record(seconds: seconds, directory: dir, apps: apps)
                exit(code)
            }

        case "--spike-transcribe":
            guard args.count == 2 else {
                fputs(usage + "\n", stderr)
                exit(64)
            }
            let file = URL(fileURLWithPath: (args[1] as NSString).expandingTildeInPath)
            Task {
                let code = await Spikes.transcribe(file: file)
                exit(code)
            }

        case "--spike-compare-asr":
            let files = args.dropFirst().map { URL(fileURLWithPath: ($0 as NSString).expandingTildeInPath) }
            guard !files.isEmpty else {
                fputs(usage + "\n", stderr)
                exit(64)
            }
            Task {
                let code = await Spikes.compareASR(files: files)
                exit(code)
            }

        case "--list-audio-processes":
            exit(Spikes.listAudioProcesses(apps: appsOption(Array(args.dropFirst()))))

        case "--list-devices":
            exit(Spikes.listDevices())

        case "--prefetch-models":
            Task {
                let code = await Spikes.prefetchModels()
                exit(code)
            }

        default:
            fputs("Argument inconnu : \(args[0])\n\n" + usage + "\n", stderr)
            exit(64)
        }

        // La boucle principale sert les notifications (veille/réveil, AVAudioEngine) et la
        // file principale ; le travail se fait dans les tâches et les files dédiées.
        RunLoop.main.run()
        exit(0)
    }

    /// Option facultative `--apps teams,slack` (seule option admise après les arguments
    /// positionnels) ; quitte avec le code 64 si elle est mal formée ou désigne une app inconnue.
    private static func appsOption(_ rest: [String]) -> [WatchedApp] {
        if rest.isEmpty { return WatchedApp.defaults }
        guard rest.count == 2, rest[0] == "--apps" else {
            fputs("Arguments inattendus : \(rest.joined(separator: " "))\n\n" + usage + "\n", stderr)
            exit(64)
        }
        let parsed = WatchedApp.fromCatalog(argument: rest[1])
        guard parsed.unknown.isEmpty else {
            let known = WatchedApp.catalog.map(\.id).joined(separator: ", ")
            fputs("App(s) inconnue(s) : \(parsed.unknown.joined(separator: ", ")). Apps connues : \(known).\n", stderr)
            exit(64)
        }
        return parsed.apps
    }

    // MARK: Mode protocole

    private enum Input {
        case line(String)
        case endOfFile
    }

    /// Sources de signaux gardées en vie pendant toute l'exécution.
    private static var signalSources: [DispatchSourceSignal] = []

    @available(macOS 14.2, *)
    private static func runProtocol() {
        let output = Output()
        let controller = Controller(output: output)

        let inputs = AsyncStream<Input>.makeStream(bufferingPolicy: .unbounded)
        let reader = Thread {
            while let line = readLine(strippingNewline: true) {
                let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
                if !trimmed.isEmpty { inputs.continuation.yield(.line(trimmed)) }
            }
            inputs.continuation.yield(.endOfFile)
            inputs.continuation.finish()
        }
        reader.name = "scribe.audio.stdin"
        reader.stackSize = 1 << 20

        for sig in [SIGTERM, SIGINT] {
            signal(sig, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: sig, queue: .global(qos: .userInitiated))
            source.setEventHandler {
                Task { await controller.shutdown(reason: "signal \(sig)") }
            }
            source.resume()
            signalSources.append(source)
        }

        Task {
            await controller.boot()
            reader.start()
            for await input in inputs.stream {
                switch input {
                case let .line(line):
                    await controller.handle(line: line)
                case .endOfFile:
                    await controller.shutdown(reason: "fin de stdin")
                }
            }
        }
    }
}
