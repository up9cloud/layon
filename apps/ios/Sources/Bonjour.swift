import Foundation

/// Resolves the agents' `_layon._udp` adverts to the endpoint id in their TXT
/// record and the `ip:port` strings `LayonClient.setAddrs` takes.
final class Bonjour: NSObject, NetServiceBrowserDelegate, NetServiceDelegate {
    var onFound: ((String, [String]) -> Void)?

    private let browser = NetServiceBrowser()
    // NetService stops resolving once nothing holds it.
    private var resolving: [NetService] = []

    func start() {
        browser.delegate = self
        browser.searchForServices(ofType: "_layon._udp.", inDomain: "local.")
    }

    func netServiceBrowser(_ browser: NetServiceBrowser, didFind service: NetService, moreComing: Bool) {
        resolving.append(service)
        service.delegate = self
        service.resolve(withTimeout: 5)
    }

    func netServiceDidResolveAddress(_ service: NetService) {
        resolving.removeAll { $0 === service }
        guard let txt = service.txtRecordData(),
              let raw = NetService.dictionary(fromTXTRecord: txt)["id"],
              let id = String(data: raw, encoding: .utf8)
        else { return }
        let addrs = (service.addresses ?? []).compactMap(Self.format)
        if !addrs.isEmpty { onFound?(id, addrs) }
    }

    func netService(_ service: NetService, didNotResolve errorDict: [String: NSNumber]) {
        resolving.removeAll { $0 === service }
    }

    private static func format(_ data: Data) -> String? {
        data.withUnsafeBytes { raw -> String? in
            guard let sa = raw.baseAddress?.assumingMemoryBound(to: sockaddr.self) else { return nil }
            var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
            var port = [CChar](repeating: 0, count: Int(NI_MAXSERV))
            guard getnameinfo(sa, socklen_t(data.count), &host, socklen_t(host.count),
                              &port, socklen_t(port.count), NI_NUMERICHOST | NI_NUMERICSERV) == 0
            else { return nil }
            let h = String(cString: host)
            let p = String(cString: port)
            // Rust's SocketAddr cannot parse a scoped address like fe80::1%en0.
            if h.contains("%") { return nil }
            return sa.pointee.sa_family == sa_family_t(AF_INET6) ? "[\(h)]:\(p)" : "\(h):\(p)"
        }
    }
}
