# Building and Running the Sherlog Ring Buffer iOS Demo App

This document provides detailed instructions on how to build and run the Sherlog Ring Buffer iOS demo application. The demo showcases all features of the ring buffer system with a visual interface.

## Prerequisites

Before you begin, make sure you have the following installed:

1. **Xcode 14.0 or later**
   - Download from the Mac App Store or the [Apple Developer website](https://developer.apple.com/xcode/)

2. **Rust and Cargo**
   - Install using rustup: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
   - Follow the instructions from the installer

3. **iOS Rust targets**
   - Add iOS targets with: `rustup target add aarch64-apple-ios x86_64-apple-ios`

4. **cargo-lipo**
   - Install with: `cargo install cargo-lipo`

## Step 1: Build the Rust Library for iOS

1. Clone the repository if you haven't already:
   ```bash
   git clone https://github.com/your-username/sherlog-ring-buffer.git
   cd sherlog-ring-buffer
   ```

2. Build the Rust library with iOS support:
   ```bash
   # Make sure you're in the project root directory
   cargo lipo --release --features ios --no-default-features
   ```

3. Create an XCFramework (recommended for distribution):
   ```bash
   mkdir -p xcframework/ios
   cp target/universal/release/libsherlog_ring_buffer.a xcframework/ios/
   
   xcodebuild -create-xcframework \
     -library xcframework/ios/libsherlog_ring_buffer.a \
     -output Sherlog.xcframework
   ```

## Step 2: Set Up the Xcode Project

1. Open the Xcode project:
   ```bash
   open examples/ios-demo/SherlongDemo/SherlongDemo.xcodeproj
   ```

2. Add the Sherlog.xcframework to your project:
   - In Xcode, select the SherlongDemo project in the Navigator
   - Go to the SherlongDemo target's "General" tab
   - Scroll to "Frameworks, Libraries, and Embedded Content"
   - Click the "+" button
   - Choose "Add Other..." and select the Sherlog.xcframework you created

3. If using the plain static library instead of XCFramework:
   - Add the library search path in the project's build settings:
   - Select the project in the Navigator
   - Go to "Build Settings" tab
   - Search for "Library Search Paths"
   - Add the path to the universal library: `$(PROJECT_DIR)/../../../target/universal/release`
   - Also add a header search path if needed

4. Create a bridging header if needed:
   - If you get import errors, create a new header file named `SherlongDemo-Bridging-Header.h`
   - Add any necessary C imports
   - Configure the project's build settings to use this bridging header

## Step 3: Build and Run the App

1. Select a Simulator or real device target in Xcode

2. Build and run the app by clicking the Play button or pressing Cmd+R

3. If you encounter any build errors:
   - Check the error messages in Xcode's Issue Navigator
   - Verify that the framework or library is correctly linked
   - Ensure the Rust build completed successfully
   - Check that the import statements match the framework name

## Troubleshooting Common Issues

### Library Not Found

If you see an error like "Library not found for -lsherlog_ring_buffer":

1. Verify the library was built correctly:
   ```bash
   ls -la target/universal/release/
   ```

2. Check that the library search paths in Xcode are correct:
   - Project → Build Settings → Library Search Paths
   - The path should point to where the .a file is located

3. Ensure the linker flags include the library:
   - Project → Build Settings → Other Linker Flags
   - Should include `-lsherlog_ring_buffer`

### Symbol Not Found

If you encounter "Symbol not found" errors:

1. Make sure you built with the correct features:
   ```bash
   cargo lipo --release --features ios --no-default-features
   ```

2. Verify the Swift-Bridge generated code is properly included:
   - Check that the Swift interface matches the Rust exports
   - Ensure any required C/Objective-C bridging is in place

3. Rebuild both the Rust library and the Xcode project from scratch:
   ```bash
   cargo clean
   cargo lipo --release --features ios --no-default-features
   ```

### Simulator Architecture Issues

For architecture-specific issues when running on the simulator:

1. Ensure you've built for both arm64 and x86_64:
   ```bash
   rustup target add aarch64-apple-ios x86_64-apple-ios
   cargo lipo --release --features ios --no-default-features
   ```

2. Check that the library supports the simulator architecture:
   ```bash
   lipo -info target/universal/release/libsherlog_ring_buffer.a
   ```

3. Set the Xcode project to build for "Any iOS Simulator Device" when testing on simulator

## Additional Development Tips

### Debugging the Rust Library

1. To debug the Rust library, build with debug symbols:
   ```bash
   cargo lipo --features ios --no-default-features
   ```

2. In Xcode, add symbolic breakpoints at function entry points

3. Use `println!` macros in Rust code (they'll appear in Xcode's console)

### Modifying the Demo App

1. The demo app is structured with:
   - **Views/**: SwiftUI view components
   - **Models/**: Data models and types
   - **Services/**: Business logic including the LoggerService
   - **Utils/**: Helper functions and extensions

2. If you make changes to the Rust library, rebuild it and update the Swift interface if needed:
   ```bash
   cargo lipo --release --features ios --no-default-features
   ```

3. When adding new features to the demo app, consider updating the README.md to reflect the changes

## Creating Your Own App Using Sherlog

To use Sherlog in your own iOS application:

1. Follow steps 1-3 to build the library and XCFramework

2. Add the framework to your app project

3. Copy the Swift interface code from `SherlogLogger.swift` in this demo

4. Initialize the logger in your app's startup code:

```swift
import SwiftUI
import Sherlog

@main
struct MyApp: App {
    @StateObject private var loggerService = LoggerService()
    
    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(loggerService)
                .onAppear {
                    // Initialize the logger
                    loggerService.initialize()
                }
                .onDisappear {
                    // Shutdown the logger
                    loggerService.shutdown()
                }
        }
    }
}
```

5. Use the logger in your views and services:

```swift
// In your view or service
@EnvironmentObject var loggerService: LoggerService

func someFunction() {
    // Log an event
    loggerService.log(message: "User performed action", level: .info)
    
    // Read logs
    let logs = loggerService.readAllLogs()
    
    // Force flush the buffer
    loggerService.forceFlush()
}
```