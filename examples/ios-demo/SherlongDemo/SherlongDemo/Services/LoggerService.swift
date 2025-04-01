import Foundation
import Combine
import Sherlog

/// Service responsible for managing the Sherlog ring buffer
class LoggerService: ObservableObject {
    // Published properties to allow UI to observe state changes
    @Published var memoryBufferUsage: Float = 0.0
    @Published var diskBufferUsage: Float = 0.0
    @Published var overflowQueueSize: Int = 0
    @Published var lastFlushTimestamp: Date?
    @Published var recentLogs: [LogRecord] = []
    @Published var isInitialized: Bool = false
    @Published var bufferStats: BufferStats = BufferStats()
    
    // Configuration
    private var volatileBufferSize: UInt32 = 1_048_576 // 1MB
    private var persistentBufferSize: UInt32 = 10_485_760 // 10MB
    private var flushInterval: UInt32 = 1000 // 1 second
    private var highWatermark: Float = 75.0 // 75%
    
    // Stats update timer
    private var statsTimer: Timer?
    
    /// Initialize the Sherlog system
    func initialize() {
        guard !isInitialized else { return }
        
        // Get document directory for log file storage
        let documentsPath = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
        let logPath = documentsPath.appendingPathComponent("sherlog_demo.dat").path
        
        // Initialize the Sherlog system
        if SherlogLogger.shared.initialize(
            diskPath: logPath,
            volatileBufferSize: volatileBufferSize,
            persistentBufferSize: persistentBufferSize,
            flushIntervalMs: flushInterval,
            highWatermarkPercent: highWatermark
        ) {
            isInitialized = true
            print("Sherlog initialized successfully at: \(logPath)")
            
            // Start timer to update stats periodically
            startStatsUpdateTimer()
        } else {
            print("Failed to initialize Sherlog")
        }
    }
    
    /// Shutdown the Sherlog system
    func shutdown() {
        guard isInitialized else { return }
        
        // Stop the stats timer
        statsTimer?.invalidate()
        statsTimer = nil
        
        // Shutdown Sherlog
        SherlogLogger.shared.shutdown()
        isInitialized = false
        print("Sherlog shut down")
    }
    
    /// Start a timer to update buffer statistics
    private func startStatsUpdateTimer() {
        statsTimer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.updateBufferStats()
        }
    }
    
    /// Update buffer statistics
    private func updateBufferStats() {
        // In a real implementation, we would get these values from the Sherlog API
        // For this demo, we'll simulate them
        memoryBufferUsage = Float.random(in: 10...50)
        diskBufferUsage = Float.random(in: 20...70)
        overflowQueueSize = Int.random(in: 0...10)
        
        if Bool.random() {
            lastFlushTimestamp = Date()
        }
        
        // Update the buffer stats object with more detailed information
        bufferStats.update(
            memoryUsage: memoryBufferUsage,
            diskUsage: diskBufferUsage,
            memoryWriteIndex: UInt64.random(in: 0...volatileBufferSize),
            memoryReadIndex: UInt64.random(in: 0...volatileBufferSize),
            memoryCommitIndex: UInt64.random(in: 0...volatileBufferSize),
            diskWritePos: UInt64.random(in: 0...persistentBufferSize),
            diskReadPos: UInt64.random(in: 0...persistentBufferSize),
            wrapCount: UInt64.random(in: 0...5),
            overflowQueueSize: overflowQueueSize,
            lastFlushTime: lastFlushTimestamp
        )
        
        // Load the most recent logs
        loadRecentLogs(5)
    }
    
    /// Write a log message
    /// - Parameters:
    ///   - message: The message to log
    ///   - level: The log level
    ///   - metadata: Optional metadata key-value pairs
    func log(message: String, level: LogLevel, metadata: [String: String] = [:]) {
        guard isInitialized else {
            print("Cannot log: Sherlog not initialized")
            return
        }
        
        // Format the log message with metadata if available
        var formattedMessage = message
        if !metadata.isEmpty {
            let metadataStr = metadata.map { key, value in "\(key): \(value)" }.joined(separator: ", ")
            formattedMessage += " [Metadata: \(metadataStr)]"
        }
        
        // Write to the Sherlog system
        if SherlogLogger.shared.write(message: formattedMessage, tag: level.rawValue) {
            print("Log written: \(level.name) - \(message)")
        } else {
            print("Failed to write log")
        }
        
        // Update stats immediately after writing
        updateBufferStats()
    }
    
    /// Load the most recent logs
    /// - Parameter count: Number of logs to load
    func loadRecentLogs(_ count: Int) {
        guard isInitialized else { return }
        
        // Use a cursor to read the latest logs
        SherlogLogger.shared.withCursor { cursor in
            var records: [LogRecord] = []
            
            // Read logs in batches
            let batch = cursor.readBatch(maxCount: UInt32(count))
            records.append(contentsOf: batch)
            
            // Update the published property on the main thread
            DispatchQueue.main.async {
                self.recentLogs = records
            }
        }
    }
    
    /// Read all available logs
    /// - Returns: Array of log records
    func readAllLogs() -> [LogRecord] {
        guard isInitialized else { return [] }
        return SherlogLogger.shared.readAllLogs()
    }
    
    /// Force a flush from memory to disk
    func forceFlush() {
        // In a real implementation, we would call the Sherlog API to force a flush
        // For this demo, we'll just update the stats
        lastFlushTimestamp = Date()
        memoryBufferUsage = Float.random(in: 0...10) // Simulate low usage after flush
        diskBufferUsage = min(diskBufferUsage + 5, 100) // Simulate increased disk usage
        
        print("Forced flush at \(lastFlushTimestamp!)")
        updateBufferStats()
    }
    
    /// Clear the memory buffer
    func clearMemoryBuffer() {
        // In a real implementation, we would call the Sherlog API to clear the memory buffer
        // For this demo, we'll just update the stats
        memoryBufferUsage = 0.0
        overflowQueueSize = 0
        
        print("Memory buffer cleared")
        updateBufferStats()
    }
    
    /// Reset the disk buffer (clear all logs)
    func resetDiskBuffer() {
        // In a real implementation, we would call the Sherlog API to reset the disk buffer
        // For this demo, we'll just update the stats
        diskBufferUsage = 0.0
        
        print("Disk buffer reset")
        updateBufferStats()
        
        // Clear the recent logs
        DispatchQueue.main.async {
            self.recentLogs = []
        }
    }
    
    /// Generate a random log for testing
    func generateRandomLog() {
        let levels: [LogLevel] = [.debug, .info, .warning, .error, .critical]
        let randomLevel = levels.randomElement()!
        
        let messages = [
            "User logged in",
            "API request completed",
            "Database query executed",
            "File upload started",
            "Cache miss detected",
            "Authentication failed",
            "Network connection lost",
            "Memory usage high",
            "Battery level low",
            "Background fetch completed"
        ]
        
        let randomMessage = messages.randomElement()!
        
        // Sometimes add metadata
        var metadata: [String: String] = [:]
        if Bool.random() {
            metadata["userId"] = "user_\(Int.random(in: 1000...9999))"
            metadata["duration"] = "\(Int.random(in: 10...500))ms"
            metadata["source"] = ["iOS", "Network", "Database", "UI"].randomElement()!
        }
        
        log(message: randomMessage, level: randomLevel, metadata: metadata)
    }
    
    /// Generate multiple random logs
    /// - Parameter count: Number of logs to generate
    func generateRandomLogs(count: Int) {
        for _ in 0..<count {
            generateRandomLog()
            
            // Small delay to simulate real-world logging patterns
            if count > 10 {
                // Use a very short delay for bulk generation
                usleep(1000) // 1ms
            }
        }
    }
    
    /// Update the buffer configuration
    /// - Parameters:
    ///   - memorySize: New memory buffer size
    ///   - diskSize: New disk buffer size
    ///   - flushMs: New flush interval in milliseconds
    ///   - watermark: New high watermark percentage
    func updateConfiguration(memorySize: UInt32? = nil, 
                            diskSize: UInt32? = nil,
                            flushMs: UInt32? = nil,
                            watermark: Float? = nil) {
        // Store the new configuration
        if let memorySize = memorySize {
            volatileBufferSize = memorySize
        }
        
        if let diskSize = diskSize {
            persistentBufferSize = diskSize
        }
        
        if let flushMs = flushMs {
            flushInterval = flushMs
        }
        
        if let watermark = watermark {
            highWatermark = watermark
        }
        
        // In a real implementation, we might need to restart the Sherlog system
        // to apply these changes. For this demo, we'll just print them.
        print("Configuration updated: memory=\(volatileBufferSize), disk=\(persistentBufferSize), flush=\(flushInterval)ms, watermark=\(highWatermark)%")
    }
}

/// Buffer statistics structure
struct BufferStats {
    var memoryUsage: Float = 0.0
    var diskUsage: Float = 0.0
    var memoryWriteIndex: UInt64 = 0
    var memoryReadIndex: UInt64 = 0
    var memoryCommitIndex: UInt64 = 0
    var diskWritePos: UInt64 = 0
    var diskReadPos: UInt64 = 0
    var wrapCount: UInt64 = 0
    var overflowQueueSize: Int = 0
    var lastFlushTime: Date?
    
    mutating func update(
        memoryUsage: Float, 
        diskUsage: Float,
        memoryWriteIndex: UInt64,
        memoryReadIndex: UInt64,
        memoryCommitIndex: UInt64,
        diskWritePos: UInt64,
        diskReadPos: UInt64,
        wrapCount: UInt64,
        overflowQueueSize: Int,
        lastFlushTime: Date?
    ) {
        self.memoryUsage = memoryUsage
        self.diskUsage = diskUsage
        self.memoryWriteIndex = memoryWriteIndex
        self.memoryReadIndex = memoryReadIndex
        self.memoryCommitIndex = memoryCommitIndex
        self.diskWritePos = diskWritePos
        self.diskReadPos = diskReadPos
        self.wrapCount = wrapCount
        self.overflowQueueSize = overflowQueueSize
        self.lastFlushTime = lastFlushTime
    }
}

/// Log level enum
enum LogLevel: UInt32 {
    case debug = 0
    case info = 1
    case warning = 2
    case error = 3
    case critical = 4
    
    var name: String {
        switch self {
        case .debug: return "DEBUG"
        case .info: return "INFO"
        case .warning: return "WARNING"
        case .error: return "ERROR"
        case .critical: return "CRITICAL"
        }
    }
    
    var color: Color {
        switch self {
        case .debug: return .gray
        case .info: return .blue
        case .warning: return .orange
        case .error: return .red
        case .critical: return .purple
        }
    }
}

// For SwiftUI previews
import SwiftUI
extension LogLevel {
    var color: Color {
        switch self {
        case .debug: return .gray
        case .info: return .blue
        case .warning: return .orange
        case .error: return .red
        case .critical: return .purple
        }
    }
}