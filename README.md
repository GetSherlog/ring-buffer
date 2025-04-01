# Sherlog Ring Buffer

A high-performance, lock-free ring buffer implementation for mobile app logging in Rust, offering:

- **Multi-Producer, Single-Consumer (MPSC)** in-memory volatile buffer
- **Single-Producer, Single-Consumer (SPSC)** persistent disk buffer
- Auto-flushing daemon to move data from memory to disk
- Cursor-based consumption for reading from persistent storage
- Native integration for both **Android (JNI)** and **iOS (Swift)**

## Overview

Sherlog Ring Buffer is a specialized logging subsystem designed for high-performance mobile applications on both Android and iOS. It provides a two-stage buffering system:

1. An in-memory ring buffer for extremely fast, lock-free writes from multiple threads
2. A persistent, memory-mapped ring buffer for durable storage on disk

A background flush daemon automatically transfers records from memory to disk based on configurable watermarks and intervals. The design enables applications to generate logs at high throughput while minimizing I/O overhead and ensuring data durability.

## Features

- **Concurrent Writes**: Multiple threads can safely write logs without locks
- **Memory-Mapped Persistence**: Fast disk I/O using memory mapping
- **Data Integrity**: CRC32 checksums and record markers to detect corruption
- **Reserve-Write-Commit Pattern**: Efficient memory management with zero-copy design
- **Overflow Protection**: Secondary queue for handling buffer-full scenarios
- **Partial Reads**: Cursor-based design for consuming arbitrary batches of records

## Architecture

The system consists of several core components:

1. **Volatile Ring Buffer**: In-memory MPSC buffer with atomic indices
2. **Persistent Ring Buffer**: Disk-backed SPSC buffer with memory mapping
3. **Flush Daemon**: Background thread moving data from memory to disk
4. **Cursor**: Stateful reader for consuming records from persistent storage
5. **Platform Bindings**: Native interfaces for Android (JNI) and iOS (Swift)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
sherlog-ring-buffer = "0.1.0"
```

### Platform Support

Sherlog Ring Buffer supports both Android and iOS platforms:

- **Android**: Using JNI bindings 
- **iOS**: Using Swift bindings

## Documentation

This project includes several documents to help you get started and make the most of Sherlog Ring Buffer:

- [**DEVELOPMENT.md**](DEVELOPMENT.md): Detailed guide for setting up development environments for Android and iOS
- [**iOS Demo App**](examples/ios-demo/README.md): Comprehensive demo showcasing all features on iOS
  - [Build Instructions](examples/ios-demo/BUILD_INSTRUCTIONS.md): Step-by-step guide to build and run the iOS demo
- [**API Documentation**](#basic-usage): Usage examples and API reference

For detailed integration and building instructions, see the [DEVELOPMENT.md](DEVELOPMENT.md) guide.

## Basic Usage

```rust
use sherlog_ring_buffer::{
    init_memory_buffer, 
    init_persistent_buffer,
    FlushDaemonConfig,
    start_flush_daemon,
    write,
    Cursor,
};

fn main() {
    // Initialize buffers
    init_memory_buffer(1024 * 1024); // 1MB volatile buffer
    init_persistent_buffer("/path/to/logs.dat", 10 * 1024 * 1024); // 10MB persistent buffer
    
    // Start the flush daemon
    let daemon = start_flush_daemon(FlushDaemonConfig::default());
    
    // Write logs from multiple threads
    write("Log message".as_bytes().to_vec(), 1); // Tag 1 for info logs
    
    // Use reservation pattern for larger messages
    if let Some(mut reservation) = sherlog_ring_buffer::reserve(100, false) {
        let data = "A longer message with more details".as_bytes();
        let mut header = sherlog_ring_buffer::types::RecordHeader::new(data.len() as u32, 2);
        header.update_crc(data);
        reservation.write(header, data);
        reservation.commit();
    }
    
    // Read logs with a cursor
    let buffer = sherlog_ring_buffer::get_persistent_buffer();
    let mut cursor = Cursor::new(buffer.clone(), None);
    
    while let Ok(Some(record)) = cursor.next() {
        println!("Log: {:?}", std::str::from_utf8(&record.data).unwrap());
    }
}
```

## Real-World Use Cases

### Logging Service for Mobile Apps

Create a logging service that captures events across your application:

```rust
// In your Rust library
pub fn log_event(level: LogLevel, message: &str, metadata: Option<HashMap<String, String>>) -> bool {
    // Serialize the log entry with its metadata
    let entry = LogEntry {
        timestamp: std::time::SystemTime::now(),
        level,
        message: message.to_string(),
        metadata: metadata.unwrap_or_default(),
    };
    
    // Serialize to binary format
    let serialized = bincode::serialize(&entry).unwrap_or_default();
    
    // Write to the buffer with the log level as tag
    sherlog_ring_buffer::write(serialized, level as u32)
}

// Can be used from Rust, or exposed through the platform bindings
```

### Crash Reporting with Context

Capture application state for crash reporting:

```rust
// In your Rust library
pub fn log_crash(exception: &str, stack_trace: &str) {
    // Get recent logs to provide context for the crash
    let logs = get_recent_logs(100); // Last 100 logs
    
    // Create a crash report with context
    let crash_report = CrashReport {
        exception: exception.to_string(),
        stack_trace: stack_trace.to_string(),
        device_info: get_device_info(),
        recent_logs: logs,
    };
    
    // Serialize and write with high priority
    let serialized = bincode::serialize(&crash_report).unwrap_or_default();
    sherlog_ring_buffer::write(serialized, PRIORITY_CRITICAL);
    
    // Force flush to ensure persistence
    flush_logs();
}
```

### Performance Monitoring

Track performance metrics across your application:

```rust
// In your Rust library
pub fn track_performance(operation: &str, duration_ms: u64, metadata: HashMap<String, String>) {
    let metric = PerformanceMetric {
        operation: operation.to_string(),
        duration_ms,
        timestamp: std::time::SystemTime::now(),
        metadata,
    };
    
    let serialized = bincode::serialize(&metric).unwrap_or_default();
    sherlog_ring_buffer::write(serialized, TAG_PERFORMANCE);
}
```

## Platform Integration

### Android Integration

```kotlin
// In your Kotlin/Java code
import com.example.app.Sherlog

// Initialize
Sherlog.initialize(
    context.filesDir.absolutePath + "/sherlog.dat",
    1 * 1024 * 1024,  // 1MB volatile buffer
    10 * 1024 * 1024, // 10MB persistent buffer
    1000,             // Flush every 1 second
    75.0f             // Or when buffer is 75% full
)

// Write logs
Sherlog.write("Log message".toByteArray(), 1)

// Read logs
val logs = Sherlog.readAllLogs()
logs.forEach { record ->
    val message = String(record.data)
    println("Log: $message")
}

// Shutdown
Sherlog.shutdown()
```

### Extended Android Example

Build a complete logging system for your Android app:

```kotlin
// Logger.kt
object Logger {
    private const val TAG_INFO = 1
    private const val TAG_WARNING = 2  
    private const val TAG_ERROR = 3
    private const val TAG_DEBUG = 4
    
    // Initialize in your Application class
    fun init(context: Context) {
        val logPath = context.filesDir.absolutePath + "/app_logs.dat"
        Sherlog.initialize(logPath, 2 * 1024 * 1024, 20 * 1024 * 1024, 500, 70.0f)
        
        // Register uncaught exception handler
        val defaultHandler = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, throwable ->
            logError("CRASH in thread ${thread.name}", throwable)
            Sherlog.shutdown() // Ensure logs are flushed
            defaultHandler?.uncaughtException(thread, throwable)
        }
    }
    
    fun debug(message: String) {
        log(message, TAG_DEBUG)
    }
    
    fun info(message: String) {
        log(message, TAG_INFO)
    }
    
    fun warning(message: String) {
        log(message, TAG_WARNING)
    }
    
    fun error(message: String, throwable: Throwable? = null) {
        val fullMessage = if (throwable != null) {
            "$message\n${throwable.stackTraceToString()}"
        } else {
            message
        }
        log(fullMessage, TAG_ERROR)
    }
    
    private fun log(message: String, tag: Int) {
        val threadInfo = "[${Thread.currentThread().name}]"
        val timestamp = SimpleDateFormat("yyyy-MM-dd HH:mm:ss.SSS", Locale.US).format(Date())
        val formattedMessage = "[$timestamp]$threadInfo: $message"
        
        Sherlog.write(formattedMessage.toByteArray(), tag)
    }
    
    // Export logs for troubleshooting
    fun exportLogs(context: Context): Uri? {
        val logs = Sherlog.readAllLogs()
        if (logs.isEmpty()) return null
        
        val outputFile = File(context.cacheDir, "logs_${System.currentTimeMillis()}.txt")
        outputFile.bufferedWriter().use { writer ->
            logs.forEach { record ->
                val prefix = when(record.tag) {
                    TAG_INFO -> "INFO"
                    TAG_WARNING -> "WARN"
                    TAG_ERROR -> "ERROR"
                    TAG_DEBUG -> "DEBUG"
                    else -> "LOG"
                }
                writer.write("$prefix: ${String(record.data)}\n")
            }
        }
        
        return FileProvider.getUriForFile(
            context,
            "${context.packageName}.fileprovider",
            outputFile
        )
    }
}
```

### iOS Integration

```swift
// In your Swift code
import Sherlog

// Initialize
let documentsPath = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
let logPath = documentsPath.appendingPathComponent("sherlog.dat").path

SherlogLogger.shared.initialize(
    diskPath: logPath,
    volatileBufferSize: 1_048_576,  // 1MB
    persistentBufferSize: 10_485_760, // 10MB
    flushIntervalMs: 1000,  // 1 second
    highWatermarkPercent: 75.0
)

// Write logs
SherlogLogger.shared.write(message: "Log message", tag: 1)

// Read logs
let logs = SherlogLogger.shared.readAllLogs()
logs.forEach { record in
    if let message = record.message {
        print("Log: \(message)")
    }
}

// Shutdown
SherlogLogger.shared.shutdown()
```

### Extended iOS Example

Create a comprehensive Swift logging framework for your iOS app:

```swift
// Logger.swift
import Foundation
import Sherlog

enum LogLevel: UInt32 {
    case debug = 0
    case info = 1
    case warning = 2
    case error = 3
    case critical = 4
    
    var prefix: String {
        switch self {
        case .debug: return "DEBUG"
        case .info: return "INFO"
        case .warning: return "WARNING"
        case .error: return "ERROR"
        case .critical: return "CRITICAL"
        }
    }
}

class Logger {
    static let shared = Logger()
    
    private let dateFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss.SSS"
        return formatter
    }()
    
    private init() {}
    
    func setup() {
        let documentsPath = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
        let logPath = documentsPath.appendingPathComponent("app_logs.dat").path
        
        SherlogLogger.shared.initialize(
            diskPath: logPath,
            volatileBufferSize: 2_097_152,  // 2MB
            persistentBufferSize: 20_971_520, // 20MB
            flushIntervalMs: 500,
            highWatermarkPercent: 70.0
        )
        
        // Set up crash handling
        setupCrashHandler()
    }
    
    private func setupCrashHandler() {
        // Register custom exception handler
        NSSetUncaughtExceptionHandler { exception in
            Logger.shared.log(
                level: .critical,
                message: "CRASH: \(exception.name.rawValue)",
                metadata: [
                    "reason": exception.reason ?? "Unknown",
                    "stackTrace": exception.callStackSymbols.joined(separator: "\n")
                ]
            )
            SherlogLogger.shared.shutdown()
        }
    }
    
    func debug(_ message: String, file: String = #file, function: String = #function, line: Int = #line) {
        log(level: .debug, message: message, file: file, function: function, line: line)
    }
    
    func info(_ message: String, file: String = #file, function: String = #function, line: Int = #line) {
        log(level: .info, message: message, file: file, function: function, line: line)
    }
    
    func warning(_ message: String, file: String = #file, function: String = #function, line: Int = #line) {
        log(level: .warning, message: message, file: file, function: function, line: line)
    }
    
    func error(_ message: String, error: Error? = nil, file: String = #file, function: String = #function, line: Int = #line) {
        var fullMessage = message
        var metadata: [String: String] = [:]
        
        if let error = error {
            metadata["error"] = "\(error)"
            
            if let nsError = error as NSError? {
                metadata["domain"] = nsError.domain
                metadata["code"] = "\(nsError.code)"
                if let reason = nsError.localizedFailureReason {
                    metadata["reason"] = reason
                }
            }
        }
        
        log(level: .error, message: fullMessage, metadata: metadata, file: file, function: function, line: line)
    }
    
    func critical(_ message: String, error: Error? = nil, file: String = #file, function: String = #function, line: Int = #line) {
        var metadata: [String: String] = [:]
        
        if let error = error {
            metadata["error"] = "\(error)"
        }
        
        log(level: .critical, message: message, metadata: metadata, file: file, function: function, line: line)
    }
    
    func log(level: LogLevel, message: String, metadata: [String: String] = [:], file: String = #file, function: String = #function, line: Int = #line) {
        let filename = URL(fileURLWithPath: file).lastPathComponent
        let timestamp = dateFormatter.string(from: Date())
        
        var logComponents = ["[\(timestamp)] [\(level.prefix)] [\(filename):\(line) \(function)]"]
        logComponents.append(message)
        
        if !metadata.isEmpty {
            let metadataStr = metadata.map { key, value in "\(key): \(value)" }.joined(separator: ", ")
            logComponents.append("[Metadata: \(metadataStr)]")
        }
        
        let logMessage = logComponents.joined(separator: " ")
        SherlogLogger.shared.write(message: logMessage, tag: level.rawValue)
    }
    
    func getLogFileURL() -> URL? {
        let documentsPath = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
        let logExportPath = documentsPath.appendingPathComponent("exported_logs_\(Int(Date().timeIntervalSince1970)).txt")
        
        do {
            let fileHandle = try FileHandle(forWritingTo: logExportPath)
            
            // Export all logs
            let logs = SherlogLogger.shared.readAllLogs()
            for record in logs {
                if let message = record.message {
                    let data = message.appending("\n").data(using: .utf8)!
                    fileHandle.write(data)
                }
            }
            
            fileHandle.closeFile()
            return logExportPath
        } catch {
            warning("Failed to export logs: \(error)")
            return nil
        }
    }
    
    func tearDown() {
        SherlogLogger.shared.shutdown()
    }
}
```

## Performance Considerations

- Buffer sizes should be powers of 2 for optimal performance
- The volatile buffer uses lock-free algorithms for high throughput
- Records larger than half the buffer size cannot be written
- Consider proper sizing based on log volume and flush frequency
- Memory mapped I/O provides good performance but has filesystem limitations

## Advanced Usage

### Structured Logging

For advanced use cases, consider implementing structured logging by serializing structured data:

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct StructuredLog {
    timestamp: u64,
    level: String,
    message: String,
    thread_id: u64,
    module: String,
    line: u32,
    // Any additional metadata fields...
    metadata: HashMap<String, String>,
}

fn log_structured(level: &str, message: &str, module: &str, line: u32, metadata: HashMap<String, String>) {
    let log = StructuredLog {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64,
        level: level.to_string(),
        message: message.to_string(),
        thread_id: std::thread::current().id().as_u64().unwrap_or(0),
        module: module.to_string(),
        line,
        metadata,
    };
    
    // Use a compact binary format like bincode or MessagePack
    let serialized = bincode::serialize(&log).unwrap_or_default();
    
    // Write with tag based on level
    let tag = match level {
        "INFO" => 1,
        "WARN" => 2,
        "ERROR" => 3,
        "DEBUG" => 4,
        _ => 0,
    };
    
    sherlog_ring_buffer::write(serialized, tag);
}
```

### Custom Data Formats

You can store any type of data in the ring buffer, not just text logs:

```rust
// Store binary telemetry data
pub fn record_telemetry(sensor_id: u32, readings: &[f32]) {
    let mut buffer = Vec::with_capacity(4 + readings.len() * 4);
    
    // Write sensor ID
    buffer.extend_from_slice(&sensor_id.to_le_bytes());
    
    // Write all readings
    for reading in readings {
        buffer.extend_from_slice(&reading.to_le_bytes());
    }
    
    // Store with tag for telemetry data
    sherlog_ring_buffer::write(buffer, TAG_TELEMETRY);
}

// Store compressed logs for efficiency
pub fn log_compressed(message: &str, level: u32) {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(message.as_bytes()).unwrap();
    let compressed = encoder.finish().unwrap();
    
    sherlog_ring_buffer::write(compressed, level);
}
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Community and Support

- **GitHub Issues**: For bug reports and feature requests, please [create an issue](https://github.com/your-username/sherlog-ring-buffer/issues)
- **Documentation**: Check the [documentation](#documentation) for detailed guides
- **Examples**: Browse the [examples directory](examples/) for more usage examples