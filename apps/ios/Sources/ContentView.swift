import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var model: Model
    @State private var scanning = false
    @State private var pasting = false
    @State private var code = ""
    @State private var forgetting: Host?

    var body: some View {
        NavigationStack {
            List {
                if !model.status.isEmpty {
                    Section { Text(model.status) }
                }
                if model.hosts.isEmpty {
                    Section {
                        Text("No paired machine. Run `layon-agent pair` on it and scan the code.")
                    }
                }
                ForEach(model.hosts, id: \.id) { host in
                    Section("\(host.name) (\(host.os))") {
                        LabeledContent("Status", value: state(host))
                        HStack {
                            Button("Sleep") { model.sleep(host) }
                            Button("Hibernate") { model.hibernate(host) }
                            Button("Wake") { model.wake(host) }
                        }
                        .buttonStyle(.bordered)
                        Button("Forget", role: .destructive) { forgetting = host }
                    }
                }
            }
            .disabled(model.busy)
            .navigationTitle("Layon")
            .toolbar {
                Menu {
                    if QRScanner.isAvailable {
                        Button("Scan QR code") { scanning = true }
                    }
                    Button("Paste code") { pasting = true }
                } label: {
                    Label("Pair", systemImage: "plus")
                }
            }
            .refreshable { await model.refresh() }
            .sheet(isPresented: $scanning) {
                QRScanner { ticket in
                    scanning = false
                    model.pair(ticket)
                }
                .ignoresSafeArea()
            }
            .alert("Pairing code", isPresented: $pasting) {
                TextField("layon://pair/…", text: $code)
                Button("Pair") {
                    model.pair(code)
                    code = ""
                }
                Button("Cancel", role: .cancel) { code = "" }
            }
            .confirmationDialog(
                "Forget this machine? Pairing again needs its QR code.",
                isPresented: Binding(get: { forgetting != nil }, set: { if !$0 { forgetting = nil } }),
                titleVisibility: .visible,
                presenting: forgetting
            ) { host in
                Button("Forget \(host.name)", role: .destructive) { model.forget(host) }
            }
        }
    }

    private func state(_ host: Host) -> String {
        switch model.awake[host.id] {
        case true?: "awake"
        case false?: "not answering"
        case nil: "checking…"
        }
    }
}
