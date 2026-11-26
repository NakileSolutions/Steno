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
      scribe-audio --spike-record <secondes> <dossier>
                                                     enregistre me.wav et them.wav (spike S1)
      scribe-audio --spike-transcribe <fichier.wav>  transcrit un fichier, temps par phrase (spike S2)
      scribe-audio --list-audio-processes            liste les processus audio vus par Core Audio
      scribe-audio --prefetch-models                 télécharge et prépare les modèles (make install)
      scribe-audio --version

    Variables d'environnement :
      SCRIBE_TAP_MODE=global   tap sur tout le son du Mac (hors Scribe) au lieu de Teams seul
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
            guard args.count == 3, let seconds = Double(args[1]), seconds > 0 else {
                fputs(usage + "\n", stderr)
                exit(64)
            }
            let dir = URL(fileURLWithPath: (args[2] as NSString).expandingTildeInPath, isDirectory: true)
            Task {
                let code = await Spikes.record(seconds: seconds, directory: dir)
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

        case "--list-audio-processes":
            exit(Spikes.listAudioProcesses())

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
