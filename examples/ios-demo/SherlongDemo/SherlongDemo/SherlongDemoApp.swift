import SwiftUI

@main
struct SherlongDemoApp: App {
    // Initialize our logger service at app startup
    @StateObject private var loggerService = LoggerService()
    
    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(loggerService)
                .onAppear {
                    // Initialize the logger when the app appears
                    loggerService.initialize()
                }
                .onDisappear {
                    // Shutdown the logger when the app disappears
                    loggerService.shutdown()
                }
        }
    }
}