import Foundation
import UIKit

@MainActor
final class Model: ObservableObject {
    @Published private(set) var hosts: [Host] = []
    @Published private(set) var awake: [String: Bool] = [:]
    @Published private(set) var status = ""
    @Published private(set) var busy = false

    private var client: LayonClient?
    private var pendingTicket: String?
    private let bonjour = Bonjour()

    func start() async {
        guard client == nil else { return }
        do {
            let dir = try FileManager.default.url(
                for: .applicationSupportDirectory, in: .userDomainMask,
                appropriateFor: nil, create: true)
            client = try await LayonClient.open(path: dir.appendingPathComponent("client.json").path)
        } catch {
            status = message(error)
            return
        }
        // iOS bars the library's own mDNS, so Bonjour feeds it addresses.
        bonjour.onFound = { [weak self] id, addrs in
            Task { @MainActor in try? await self?.client?.setAddrs(id: id, addrs: addrs) }
        }
        bonjour.start()
        await refresh()
        if let ticket = pendingTicket {
            pendingTicket = nil
            pair(ticket)
        }
    }

    func refresh() async {
        guard let client else { return }
        hosts = await client.hosts()
        for host in hosts {
            Task {
                do {
                    try await client.ping(id: host.id)
                    awake[host.id] = true
                } catch {
                    awake[host.id] = false
                }
            }
        }
    }

    func pair(_ ticket: String) {
        guard client != nil else {
            pendingTicket = ticket
            return
        }
        let name = UIDevice.current.name
        run("Pairing…", done: "Paired.") { client in
            _ = try await client.pair(ticket: ticket.trimmingCharacters(in: .whitespacesAndNewlines), deviceName: name)
        }
    }

    func sleep(_ host: Host) {
        run("Putting \(host.name) to sleep…") { try await $0.power(id: host.id, action: .sleep) }
    }

    func hibernate(_ host: Host) {
        run("Hibernating \(host.name)…") { try await $0.power(id: host.id, action: .hibernate) }
    }

    func wake(_ host: Host) {
        run("Waking \(host.name)…", done: "Awake.") { client in
            let deadline = Date().addingTimeInterval(60)
            while true {
                try await client.wake(id: host.id)
                if (try? await client.ping(id: host.id)) != nil { return }
                if Date() > deadline {
                    throw LayonError.Failed(message: "\(host.name) did not come back within a minute")
                }
            }
        }
    }

    func forget(_ host: Host) {
        run("Forgetting \(host.name)…") { try await $0.forget(id: host.id) }
    }

    private func run(_ doing: String, done: String = "Done.", _ op: @escaping (LayonClient) async throws -> Void) {
        guard let client, !busy else { return }
        busy = true
        status = doing
        Task {
            do {
                try await op(client)
                status = done
            } catch {
                status = message(error)
            }
            busy = false
            await refresh()
        }
    }

    private func message(_ error: Error) -> String {
        if case LayonError.Failed(let message) = error { return message }
        return error.localizedDescription
    }
}
