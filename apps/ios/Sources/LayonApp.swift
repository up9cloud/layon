import SwiftUI

@main
struct LayonApp: App {
    @StateObject private var model = Model()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(model)
                .task { await model.start() }
                // The system Camera opens layon://pair/... links here.
                .onOpenURL { model.pair($0.absoluteString) }
        }
    }
}
