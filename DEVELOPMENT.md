# Development Guide for Sherlog Ring Buffer

This guide covers how to set up development environments for both Android and iOS platforms, build the library for each platform, and use the bindings in your application code.

## Prerequisites

- Rust and Cargo (latest stable version)
- For Android: Android NDK and cargo-ndk
- For iOS: Xcode and cargo-lipo

## Setting Up the Development Environment

### Common Setup

1. Install Rust and Cargo:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Clone the repository:
   ```bash
   git clone https://github.com/your-org/sherlog-ring-buffer.git
   cd sherlog-ring-buffer
   ```

### Android Setup

1. Install the Android NDK through Android Studio or directly.

2. Install cargo-ndk:
   ```bash
   cargo install cargo-ndk
   ```

3. Add Android targets to Rust:
   ```bash
   rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
   ```

4. Configure your environment to locate the NDK:
   ```bash
   export ANDROID_NDK_HOME=/path/to/android/ndk
   ```

### iOS Setup

1. Install Xcode from the Mac App Store.

2. Install cargo-lipo for universal iOS binary generation:
   ```bash
   cargo install cargo-lipo
   ```

3. Add iOS targets to Rust:
   ```bash
   rustup target add aarch64-apple-ios x86_64-apple-ios
   ```

## Building the Library

### Building for Android

1. Use cargo-ndk to build for specific Android architectures:

   ```bash
   # For ARM64
   cargo ndk --target aarch64-linux-android --platform 21 build --release
   
   # For ARMv7
   cargo ndk --target armv7-linux-androideabi --platform 21 build --release
   
   # For x86
   cargo ndk --target i686-linux-android --platform 21 build --release
   
   # For x86_64
   cargo ndk --target x86_64-linux-android --platform 21 build --release
   ```

2. The compiled libraries will be in `target/<target>/release/libsherlog_ring_buffer.so`.

### Building for iOS

1. Build a universal binary with cargo-lipo:

   ```bash
   cargo lipo --release --features ios --no-default-features
   ```

2. Create an XCFramework for distribution:

   ```bash
   mkdir -p xcframework/ios
   cp target/universal/release/libsherlog_ring_buffer.a xcframework/ios/
   
   xcodebuild -create-xcframework \
     -library xcframework/ios/libsherlog_ring_buffer.a \
     -output Sherlog.xcframework
   ```

## Integration with Your Application

### Android Integration

1. Add the native library to your Gradle build:

   ```gradle
   // In app/build.gradle
   android {
       // ...
       defaultConfig {
           // ...
           externalNativeBuild {
               cmake {
                   arguments "-DANDROID_STL=c++_shared"
               }
           }
       }
       
       // Set up paths to the compiled .so files
       sourceSets {
           main {
               jniLibs.srcDirs = ['src/main/jniLibs']
           }
       }
   }
   ```

2. Copy the compiled .so files to your project's jniLibs directory:

   ```
   app/src/main/jniLibs/
   ├── arm64-v8a/
   │   └── libsherlog_ring_buffer.so
   ├── armeabi-v7a/
   │   └── libsherlog_ring_buffer.so
   ├── x86/
   │   └── libsherlog_ring_buffer.so
   └── x86_64/
       └── libsherlog_ring_buffer.so
   ```

3. Create the Java/Kotlin interface class:

   ```kotlin
   // Create file at app/src/main/java/com/example/app/Sherlog.kt
   package com.example.app

   data class LogRecord(
       val data: ByteArray,
       val crc32: Long,
       val tag: Int,
       val timestampUs: Long
   ) {
       override fun equals(other: Any?): Boolean {
           if (this === other) return true
           if (javaClass != other?.javaClass) return false

           other as LogRecord

           if (!data.contentEquals(other.data)) return false
           if (crc32 != other.crc32) return false
           if (tag != other.tag) return false
           if (timestampUs != other.timestampUs) return false

           return true
       }

       override fun hashCode(): Int {
           var result = data.contentHashCode()
           result = 31 * result + crc32.hashCode()
           result = 31 * result + tag
           result = 31 * result + timestampUs.hashCode()
           return result
       }
   }

   object Sherlog {
       init {
           System.loadLibrary("sherlog_ring_buffer")
       }

       external fun initialize(
           diskPath: String,
           volatileBufferSize: Int,
           persistentBufferSize: Int,
           flushIntervalMs: Int,
           highWatermarkPercent: Float
       ): Boolean

       external fun shutdown()

       external fun write(data: ByteArray, tag: Int): Boolean

       external fun createCursor(): Long

       external fun readBatch(cursorPtr: Long, maxCount: Int): Array<LogRecord>?

       external fun closeCursor(cursorPtr: Long)
       
       inline fun <T> withCursor(block: (Long) -> T): T {
           val cursor = createCursor()
           try {
               return block(cursor)
           } finally {
               closeCursor(cursor)
           }
       }
       
       fun readAllLogs(): List<LogRecord> {
           val result = mutableListOf<LogRecord>()
           
           withCursor { cursor ->
               var batch: Array<LogRecord>?
               do {
                   batch = readBatch(cursor, 100)
                   batch?.let { result.addAll(it) }
               } while (batch != null && batch.isNotEmpty())
           }
           
           return result
       }
   }
   ```

### iOS Integration

1. Add the Sherlog.xcframework to your Xcode project:
   - Drag and drop the framework into your project
   - In the "Frameworks, Libraries, and Embedded Content" section, ensure it's set to "Embed & Sign"

2. Create a Swift wrapper:

   ```swift
   // Create file Sherlog.swift
   import Foundation
   import Sherlog

   // LogRecord represents a single entry in the log buffer
   public struct LogRecord {
       /// Raw binary data for the log record
       public let data: Data
       
       /// CRC32 checksum of the data
       public let crc32: UInt32
       
       /// Timestamp in microseconds since epoch
       public let timestampUs: UInt64
       
       /// Tag value for categorizing records
       public let tag: UInt32
       
       /// Convenience property to access the data as a UTF8 string
       public var message: String? {
           return String(data: data, encoding: .utf8)
       }
   }

   // SherlongLogger provides access to the Rust-based ring buffer logging system
   public class SherlongLogger {
       
       /// Singleton instance
       public static let shared = SherlongLogger()
       
       private init() {}
       
       // Initialize the logging system
       public func initialize(
           diskPath: String,
           volatileBufferSize: UInt32 = 1_048_576, // 1MB
           persistentBufferSize: UInt32 = 10_485_760, // 10MB
           flushIntervalMs: UInt32 = 1000,
           highWatermarkPercent: Float = 75.0
       ) -> Bool {
           return SherlongBindings.initialize(
               diskPath,
               volatileBufferSize: volatileBufferSize,
               persistentBufferSize: persistentBufferSize,
               flushIntervalMs: flushIntervalMs,
               highWatermarkPercent: highWatermarkPercent
           )
       }
       
       // Shutdown the logging system
       public func shutdown() {
           SherlongBindings.shutdown()
       }
       
       // Write a string message to the log
       public func write(message: String, tag: UInt32) -> Bool {
           guard let data = message.data(using: .utf8) else {
               return false
           }
           return SherlongBindings.writeLogData(data, tag: tag)
       }
       
       // Write raw data to the log
       public func write(data: Data, tag: UInt32) -> Bool {
           return SherlongBindings.writeLogData(data, tag: tag)
       }
       
       // Read all available logs
       public func readAllLogs() -> [LogRecord] {
           var cursor = SherlongBindings.createCursor()
           var records = [LogRecord]()
           
           while let record = SherlongBindings.readNextRecord(&cursor) {
               records.append(record)
           }
           
           _ = SherlongBindings.commitCursor(cursor)
           return records
       }
       
       // Read logs with a cursor
       public func withCursor(_ block: (LogCursor) -> Void) {
           let rustCursor = SherlongBindings.createCursor()
           let cursor = LogCursor(rustCursor: rustCursor)
           block(cursor)
       }
   }

   // LogCursor provides a way to read logs incrementally
   public class LogCursor {
       private var rustCursor: SherlongCursor
       
       internal init(rustCursor: SherlongCursor) {
           self.rustCursor = rustCursor
       }
       
       deinit {
           commit()
       }
       
       // Read the next record
       public func next() -> LogRecord? {
           return SherlongBindings.readNextRecord(&rustCursor)
       }
       
       // Read a batch of records
       public func readBatch(maxCount: UInt32) -> [LogRecord] {
           return SherlongBindings.readBatch(&rustCursor, maxCount: maxCount)
       }
       
       // Reset the cursor
       public func reset() {
           SherlongBindings.resetCursor(&rustCursor)
       }
       
       // Commit the cursor position
       @discardableResult
       public func commit() -> Bool {
           return SherlongBindings.commitCursor(rustCursor)
       }
   }
   ```

## Usage Examples

### Android Usage

```kotlin
import com.example.app.Sherlog
import android.content.Context

class YourApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        
        // Initialize
        val logPath = filesDir.absolutePath + "/sherlog.dat"
        Sherlog.initialize(
            logPath,
            1 * 1024 * 1024,  // 1MB volatile buffer
            10 * 1024 * 1024, // 10MB persistent buffer
            1000,             // Flush every 1 second
            75.0f             // Or when buffer is 75% full
        )
    }
    
    override fun onTerminate() {
        // Shutdown properly
        Sherlog.shutdown()
        super.onTerminate()
    }
}

// Writing logs
fun logInfo(message: String) {
    Sherlog.write(message.toByteArray(), 1) // Tag 1 for INFO
}

fun logError(message: String, throwable: Throwable) {
    val fullMessage = "$message\n${throwable.stackTraceToString()}"
    Sherlog.write(fullMessage.toByteArray(), 3) // Tag 3 for ERROR
}

// Reading logs
fun displayLogs() {
    val logs = Sherlog.readAllLogs()
    logs.forEach { record ->
        val message = String(record.data)
        val timestamp = Date(record.timestampUs / 1000) // Convert us to ms
        
        when (record.tag) {
            1 -> println("INFO [$timestamp]: $message")
            2 -> println("WARN [$timestamp]: $message")
            3 -> println("ERROR [$timestamp]: $message")
            else -> println("LOG [$timestamp]: $message")
        }
    }
}
```

### iOS Usage

```swift
import Foundation
import Sherlog

class LoggingService {
    static let shared = LoggingService()
    
    private init() {
        // Get documents directory for log file
        let documentsPath = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
        let logPath = documentsPath.appendingPathComponent("sherlog.dat").path
        
        // Initialize the logging system
        SherlongLogger.shared.initialize(
            diskPath: logPath,
            volatileBufferSize: 1_048_576,  // 1MB
            persistentBufferSize: 10_485_760, // 10MB
            flushIntervalMs: 1000,  // 1 second
            highWatermarkPercent: 75.0
        )
    }
    
    deinit {
        SherlongLogger.shared.shutdown()
    }
    
    enum LogLevel: UInt32 {
        case info = 1
        case warning = 2
        case error = 3
    }
    
    func log(_ message: String, level: LogLevel) {
        SherlongLogger.shared.write(message: message, tag: level.rawValue)
    }
    
    func logInfo(_ message: String) {
        log(message, level: .info)
    }
    
    func logWarning(_ message: String) {
        log(message, level: .warning)
    }
    
    func logError(_ message: String, error: Error? = nil) {
        var fullMessage = message
        if let error = error {
            fullMessage += "\n\(error)"
        }
        log(fullMessage, level: .error)
    }
    
    func getLogs() -> [LogRecord] {
        return SherlongLogger.shared.readAllLogs()
    }
    
    func displayLogs() {
        let logs = getLogs()
        
        for record in logs {
            let message = record.message ?? "[Binary data]"
            let date = Date(timeIntervalSince1970: TimeInterval(record.timestampUs) / 1_000_000)
            let formatter = DateFormatter()
            formatter.dateFormat = "yyyy-MM-dd HH:mm:ss.SSS"
            let timestamp = formatter.string(from: date)
            
            switch record.tag {
            case 1:
                print("INFO [\(timestamp)]: \(message)")
            case 2:
                print("WARN [\(timestamp)]: \(message)")
            case 3:
                print("ERROR [\(timestamp)]: \(message)")
            default:
                print("LOG [\(timestamp)]: \(message)")
            }
        }
    }
}

// Usage
func applicationDidFinishLaunching() {
    // Already initialized in shared instance
    LoggingService.shared.logInfo("Application started")
}

func someFunction() {
    do {
        // Some operation
        LoggingService.shared.logInfo("Operation completed successfully")
    } catch let error {
        LoggingService.shared.logError("Operation failed", error: error)
    }
}
```

## Troubleshooting Common Issues

### Android

1. **Library not loaded error**:
   - Ensure the `.so` files are in the correct jniLibs directories
   - Check that the library name in `System.loadLibrary()` matches the compiled library name

2. **JNI method not found**:
   - Verify that the method signatures match between Rust and Kotlin/Java
   - Ensure the package name in Rust JNI functions matches your app's package

### iOS

1. **Symbol not found errors**:
   - Make sure the Swift-Bridge generated code is properly included in your project
   - Verify that the binary was built with the correct architecture for your device/simulator

2. **Memory management issues**:
   - Ensure cursor objects are properly closed when finished
   - Use the `withCursor` pattern to automatically manage cursor lifecycle

## Performance Tips

1. **Buffer Sizing**:
   - Use power-of-2 sizes for optimal performance
   - Volatile buffer should be sized based on peak logging rate and flush frequency
   - Persistent buffer should be sized based on retention requirements

2. **Batch Operations**:
   - Use batch reading when processing large numbers of logs
   - Consider compressing or aggregating logs before writing for high-volume scenarios

3. **Minimize String Conversions**:
   - For maximum performance, work with binary data directly when possible
   - Pre-format strings before logging to minimize processing during write operations