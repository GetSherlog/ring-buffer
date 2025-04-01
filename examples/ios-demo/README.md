# Sherlog Ring Buffer iOS Demo App

This demo app provides a comprehensive showcase of the Sherlog Ring Buffer's capabilities on iOS. It features a visual interface to monitor, debug, and interact with all aspects of the ring buffer system.

## Features

- Real-time visualization of buffer states (memory and disk)
- Log viewer with filtering and search capabilities
- Manual and automatic log generation
- Stress testing with configurable parameters
- Buffer statistics and performance metrics
- Crash simulation and recovery demonstration
- Export and share logs functionality

## Screenshots

![Dashboard](screenshots/dashboard.png)
![Log Viewer](screenshots/log_viewer.png)
![Buffer Stats](screenshots/buffer_stats.png)
![Settings](screenshots/settings.png)

## Prerequisites

- Xcode 14.0 or later
- iOS 15.0+ deployment target
- Rust toolchain with iOS targets
- cargo-lipo installed

## Building the Demo App

### Step 1: Build the Rust Library for iOS

1. First, make sure you have the necessary Rust targets installed:

```bash
rustup target add aarch64-apple-ios x86_64-apple-ios
```

2. Install cargo-lipo if you haven't already:

```bash
cargo install cargo-lipo
```

3. Navigate to the Sherlog Ring Buffer project root and build the library for iOS:

```bash
cd /path/to/sherlog-ring-buffer
cargo lipo --release --features ios --no-default-features
```

4. Create an XCFramework (optional, but recommended for distribution):

```bash
mkdir -p xcframework/ios
cp target/universal/release/libsherlog_ring_buffer.a xcframework/ios/

xcodebuild -create-xcframework \
  -library xcframework/ios/libsherlog_ring_buffer.a \
  -output Sherlog.xcframework
```

### Step 2: Open and Build the Xcode Project

1. Open the iOS demo app project in Xcode:

```bash
open sherlog-ring-buffer/examples/ios-demo/SherlongDemo.xcodeproj
```

2. If you created an XCFramework, add it to the project:
   - In Xcode, select the project in the Navigator
   - Go to the target's "General" tab
   - Scroll to "Frameworks, Libraries, and Embedded Content"
   - Click the "+" button and add your Sherlog.xcframework

3. Build and run the project on a simulator or device

## Using the Demo App

### Dashboard

The dashboard provides an overview of the ring buffer system with real-time metrics:

- **Memory Buffer**: Visual representation of the volatile buffer
  - Shows current usage (%)
  - Write index position
  - Read index position
  - Commit index position
  - Number of items in overflow queue

- **Disk Buffer**: Visual representation of the persistent buffer
  - Shows current usage (%)
  - Write position
  - Read position
  - Wrap count
  - Last flush timestamp

- **Quick Actions**:
  - Generate random logs
  - Clear memory buffer
  - Force flush to disk
  - Reset disk buffer

### Log Generator

Use this screen to manually or automatically generate logs:

- **Manual Log Entry**:
  - Select log level (Debug, Info, Warning, Error, Critical)
  - Enter log message
  - Add custom metadata (key-value pairs)
  - Submit button writes to the ring buffer

- **Automatic Generator**:
  - Configure generation rate (logs per second)
  - Set distribution of log levels
  - Configure size range (random message size)
  - Start/stop controls for continuous generation
  - Progress and stats display

### Log Viewer

Browse and search logs stored in the persistent buffer:

- Table view of all logs with:
  - Timestamp
  - Log level (color-coded)
  - Message preview
  - Tap for detailed view

- **Filtering Options**:
  - By log level
  - By date range
  - By text content
  - By metadata fields

- **Actions**:
  - Share button for exporting visible logs
  - Clear button for resetting the buffer
  - Refresh button to reload from disk

### Buffer Inspector

Detailed visualization and configuration of both buffers:

- **Memory Buffer Inspector**:
  - Byte-level visualization of buffer contents
  - Current indices and wrap status
  - Memory address display
  - Record boundary markers

- **Disk Buffer Inspector**:
  - File structure visualization
  - Control block details
  - Record header inspection
  - Wrap detection display

- **Configuration**:
  - Adjust buffer sizes
  - Set flush parameters
  - Configure high watermark percentage
  - Enable/disable debug features

### Stress Test

Tool for performance testing the ring buffer under load:

- **Test Parameters**:
  - Number of concurrent threads
  - Logs per thread
  - Log size (fixed or random)
  - Log level distribution
  - Run duration

- **Results Display**:
  - Throughput (logs/second)
  - Latency statistics (min/avg/max/p99)
  - Buffer overflow tracking
  - CPU/Memory usage monitoring

### Crash Simulator

Demonstrate crash recovery capabilities:

- **Crash Scenarios**:
  - Simulated app crash during write
  - Force-quit during flush
  - Low memory termination
  - Out-of-bounds buffer access

- **Recovery Demonstration**:
  - Buffer integrity checking
  - Data recovery after crash
  - Orphaned record detection
  - Automatic repair mechanisms

## Customizing the Demo

### Configuration File

The app includes a `Config.plist` file where you can modify various parameters:

- Default buffer sizes
- UI refresh rates
- Test parameters
- Sample data generators
- Feature toggles

### Adding Your Own Test Cases

You can extend the demo with your own test cases by:

1. Creating a new Swift file in the project
2. Implementing the `TestCase` protocol
3. Registering your test in the `TestRegistry`

Example:

```swift
import Foundation

class MyCustomTest: TestCase {
    var name: String = "My Custom Test"
    var description: String = "Demonstrates a custom usage pattern"
    
    func run(completion: @escaping (TestResult) -> Void) {
        // Your test implementation
        // ...
        
        completion(TestResult(
            success: true,
            metrics: ["throughput": 1000, "latency": 5.2],
            message: "Test completed successfully"
        ))
    }
}

// In AppDelegate or early initialization code
TestRegistry.shared.register(MyCustomTest())
```

## Troubleshooting

### Common Issues

1. **Library Not Found**:
   - Ensure the Rust library was built correctly with the iOS feature
   - Check that the library is properly linked in Xcode
   - Verify the `LIBRARY_SEARCH_PATHS` in build settings

2. **Symbol Not Found Errors**:
   - Make sure you're building with the `ios` feature flag
   - Check that Swift-Bridge generated the correct bindings
   - Rebuild the library with `cargo clean` first

3. **App Crashes on Start**:
   - Verify that all required iOS targets were built
   - Check the linking phase in Xcode
   - Look for missing frameworks or dependencies

### Getting Help

If you encounter issues not covered here:

1. Check the logs in Xcode's console for detailed error messages
2. Review the Swift-Bridge generated code for any issues
3. File an issue in the GitHub repository with:
   - Detailed description of the problem
   - Steps to reproduce
   - Xcode and iOS version information
   - Build logs and error messages

## License

This demo app is released under the same MIT License as the Sherlog Ring Buffer library.