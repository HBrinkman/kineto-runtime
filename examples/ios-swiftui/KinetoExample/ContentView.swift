import KinetoRuntime
import SwiftUI

/// Loads the bundled `sample.kvk`, plays it in `KinetoPlayerViewController`
/// and lists the document's state machines and inputs.
struct ContentView: View {
    @State private var loaded: Loaded?
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Group {
                if let loaded {
                    VStack(spacing: 0) {
                        KinetoPlayerView(player: loaded.player)
                            .aspectRatio(4 / 3, contentMode: .fit)
                        List {
                            Section("State machines") {
                                if loaded.machines.isEmpty {
                                    Text("none")
                                }
                                ForEach(loaded.machines) { machine in
                                    VStack(alignment: .leading) {
                                        Text(machine.name).font(.headline)
                                        ForEach(loaded.player.inputs(in: machine.id)) { input in
                                            Text("\(input.name) (\(String(describing: input.kind)))")
                                                .font(.caption)
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if let error {
                    Text(error).foregroundStyle(.red).padding()
                } else {
                    ProgressView()
                }
            }
            .navigationTitle("Kineto")
        }
        .task { load() }
    }

    @MainActor
    private func load() {
        do {
            guard let url = Bundle.main.url(forResource: "sample", withExtension: "kvk") else {
                error = "sample.kvk is missing from the app bundle"
                return
            }
            let player = try KinetoPlayer(contentsOf: url)
            loaded = Loaded(player: player, machines: player.stateMachines())
        } catch {
            self.error = String(describing: error)
        }
    }
}

private struct Loaded {
    let player: KinetoPlayer
    let machines: [KinetoPlayer.MachineInfo]
}

/// SwiftUI wrapper around the runtime's Metal view controller.
private struct KinetoPlayerView: UIViewControllerRepresentable {
    let player: KinetoPlayer

    func makeUIViewController(context: Context) -> KinetoPlayerViewController {
        KinetoPlayerViewController(player: player)
    }

    func updateUIViewController(_ controller: KinetoPlayerViewController, context: Context) {}
}
