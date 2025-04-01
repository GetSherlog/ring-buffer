import SwiftUI

struct ContentView: View {
    @EnvironmentObject var loggerService: LoggerService
    @State private var selectedTab = 0
    
    var body: some View {
        TabView(selection: $selectedTab) {
            DashboardView()
                .tabItem {
                    Label("Dashboard", systemImage: "gauge")
                }
                .tag(0)
            
            LogGeneratorView()
                .tabItem {
                    Label("Generate", systemImage: "plus.circle")
                }
                .tag(1)
            
            LogViewerView()
                .tabItem {
                    Label("Logs", systemImage: "list.bullet")
                }
                .tag(2)
            
            BufferInspectorView()
                .tabItem {
                    Label("Buffers", systemImage: "cylinder.split.1x2")
                }
                .tag(3)
            
            SettingsView()
                .tabItem {
                    Label("Settings", systemImage: "gear")
                }
                .tag(4)
        }
    }
}

// Dashboard View showing buffer status and quick actions
struct DashboardView: View {
    @EnvironmentObject var loggerService: LoggerService
    
    var body: some View {
        NavigationView {
            ScrollView {
                VStack(spacing: 20) {
                    // Status cards
                    HStack(spacing: 15) {
                        StatusCard(
                            title: "Memory Buffer",
                            value: "\(Int(loggerService.memoryBufferUsage))%",
                            systemImage: "memorychip",
                            color: .blue
                        )
                        
                        StatusCard(
                            title: "Disk Buffer",
                            value: "\(Int(loggerService.diskBufferUsage))%",
                            systemImage: "internaldrive",
                            color: .green
                        )
                    }
                    
                    // Buffer visualizations
                    VStack(spacing: 20) {
                        BufferVisualization(
                            title: "Memory Buffer",
                            percentage: loggerService.memoryBufferUsage,
                            color: .blue
                        )
                        
                        BufferVisualization(
                            title: "Disk Buffer",
                            percentage: loggerService.diskBufferUsage,
                            color: .green
                        )
                    }
                    .padding()
                    .background(Color(.systemBackground))
                    .cornerRadius(10)
                    .shadow(radius: 2)
                    
                    // Quick actions
                    VStack(alignment: .leading) {
                        Text("Quick Actions")
                            .font(.headline)
                            .padding(.bottom, 10)
                        
                        HStack {
                            ActionButton(title: "Generate Logs", systemImage: "plus.circle") {
                                loggerService.generateRandomLogs(count: 5)
                            }
                            
                            ActionButton(title: "Force Flush", systemImage: "arrow.down.doc") {
                                loggerService.forceFlush()
                            }
                        }
                        
                        HStack {
                            ActionButton(title: "Clear Memory", systemImage: "trash") {
                                loggerService.clearMemoryBuffer()
                            }
                            
                            ActionButton(title: "Reset Disk", systemImage: "arrow.clockwise") {
                                loggerService.resetDiskBuffer()
                            }
                        }
                    }
                    .padding()
                    .background(Color(.systemBackground))
                    .cornerRadius(10)
                    .shadow(radius: 2)
                    
                    // Recent logs
                    VStack(alignment: .leading) {
                        Text("Recent Logs")
                            .font(.headline)
                            .padding(.bottom, 10)
                        
                        if loggerService.recentLogs.isEmpty {
                            Text("No recent logs")
                                .foregroundColor(.secondary)
                                .frame(maxWidth: .infinity, alignment: .center)
                                .padding()
                        } else {
                            ForEach(loggerService.recentLogs.prefix(3), id: \.timestampUs) { log in
                                LogRow(log: log)
                            }
                        }
                    }
                    .padding()
                    .background(Color(.systemBackground))
                    .cornerRadius(10)
                    .shadow(radius: 2)
                }
                .padding()
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle("Sherlog Dashboard")
        }
    }
}

// Log Generator View for creating test logs
struct LogGeneratorView: View {
    @EnvironmentObject var loggerService: LoggerService
    @State private var message = ""
    @State private var selectedLevel: LogLevel = .info
    @State private var metadata: [String: String] = [:]
    @State private var key = ""
    @State private var value = ""
    @State private var isGenerating = false
    @State private var generationCount = 10
    @State private var generationRate = 1.0
    @State private var bulkProgress: Double = 0
    @State private var timer: Timer?
    
    var body: some View {
        NavigationView {
            Form {
                Section(header: Text("Manual Log Entry")) {
                    Picker("Log Level", selection: $selectedLevel) {
                        Text("Debug").tag(LogLevel.debug)
                        Text("Info").tag(LogLevel.info)
                        Text("Warning").tag(LogLevel.warning)
                        Text("Error").tag(LogLevel.error)
                        Text("Critical").tag(LogLevel.critical)
                    }
                    .pickerStyle(SegmentedPickerStyle())
                    
                    TextField("Log message", text: $message)
                    
                    HStack {
                        TextField("Key", text: $key)
                        TextField("Value", text: $value)
                        Button(action: addMetadata) {
                            Image(systemName: "plus.circle")
                        }
                    }
                    
                    if !metadata.isEmpty {
                        ForEach(metadata.sorted(by: { $0.key < $1.key }), id: \.key) { key, value in
                            HStack {
                                Text(key).bold()
                                Spacer()
                                Text(value)
                                Button(action: { removeMetadata(key: key) }) {
                                    Image(systemName: "minus.circle")
                                        .foregroundColor(.red)
                                }
                            }
                        }
                    }
                    
                    Button("Submit Log") {
                        submitLog()
                    }
                    .buttonStyle(BorderedProminentButtonStyle())
                    .disabled(message.isEmpty)
                }
                
                Section(header: Text("Automatic Generation")) {
                    Stepper("Count: \(generationCount)", value: $generationCount, in: 1...1000)
                    
                    HStack {
                        Text("Rate: \(generationRate, specifier: "%.1f") logs/sec")
                        Slider(value: $generationRate, in: 0.1...20)
                    }
                    
                    if isGenerating {
                        ProgressView(value: bulkProgress)
                            .progressViewStyle(LinearProgressViewStyle())
                        
                        Button("Stop Generation") {
                            stopGeneration()
                        }
                        .buttonStyle(BorderedProminentButtonStyle())
                        .tint(.red)
                    } else {
                        Button("Start Generation") {
                            startGeneration()
                        }
                        .buttonStyle(BorderedProminentButtonStyle())
                    }
                }
                
                Section(header: Text("Stress Test")) {
                    Button("Generate 100 Logs") {
                        bulkGenerate(count: 100)
                    }
                    
                    Button("Generate 1,000 Logs") {
                        bulkGenerate(count: 1000)
                    }
                    
                    Button("Simulate Crash") {
                        simulateCrash()
                    }
                    .foregroundColor(.red)
                }
            }
            .navigationTitle("Log Generator")
        }
    }
    
    private func addMetadata() {
        guard !key.isEmpty else { return }
        metadata[key] = value
        key = ""
        value = ""
    }
    
    private func removeMetadata(key: String) {
        metadata.removeValue(forKey: key)
    }
    
    private func submitLog() {
        guard !message.isEmpty else { return }
        loggerService.log(message: message, level: selectedLevel, metadata: metadata)
        message = ""
        metadata = [:]
    }
    
    private func startGeneration() {
        isGenerating = true
        bulkProgress = 0
        
        timer = Timer.scheduledTimer(withTimeInterval: 1.0 / generationRate, repeats: true) { timer in
            loggerService.generateRandomLog()
            bulkProgress += 1.0 / Double(generationCount)
            
            if bulkProgress >= 1.0 {
                stopGeneration()
            }
        }
    }
    
    private func stopGeneration() {
        timer?.invalidate()
        timer = nil
        isGenerating = false
    }
    
    private func bulkGenerate(count: Int) {
        // Use a background thread for bulk generation
        DispatchQueue.global(qos: .userInitiated).async {
            // Update UI on main thread
            DispatchQueue.main.async {
                self.isGenerating = true
                self.bulkProgress = 0
            }
            
            // Generate logs in batches to update progress
            let batchSize = 10
            let batchCount = count / batchSize
            
            for i in 0..<batchCount {
                // Generate a batch
                self.loggerService.generateRandomLogs(count: batchSize)
                
                // Update progress on main thread
                DispatchQueue.main.async {
                    self.bulkProgress = Double(i + 1) / Double(batchCount)
                }
                
                // Give UI time to update
                usleep(10000) // 10ms
            }
            
            // Complete
            DispatchQueue.main.async {
                self.isGenerating = false
                self.bulkProgress = 1.0
            }
        }
    }
    
    private func simulateCrash() {
        // Log that we're about to crash
        loggerService.log(
            message: "Simulating application crash...",
            level: .critical,
            metadata: ["simulation": "true", "function": "simulateCrash()"]
        )
        
        // Force flush to ensure the log is written
        loggerService.forceFlush()
        
        // In a real crash scenario, the app would terminate.
        // For our demo, we'll just show a simulated crash
        // and demonstrate recovery of the log entries.
        loggerService.log(
            message: "Application recovered from simulated crash",
            level: .info,
            metadata: ["simulation": "true", "recovery": "successful"]
        )
    }
}

// Log Viewer View for browsing logs
struct LogViewerView: View {
    @EnvironmentObject var loggerService: LoggerService
    @State private var logs: [LogRecord] = []
    @State private var searchText = ""
    @State private var selectedLevels: Set<LogLevel> = [.debug, .info, .warning, .error, .critical]
    @State private var selectedLog: LogRecord?
    @State private var showDetailSheet = false
    
    var filteredLogs: [LogRecord] {
        logs.filter { log in
            // Filter by selected levels
            guard let level = LogLevel(rawValue: log.tag) else { return false }
            guard selectedLevels.contains(level) else { return false }
            
            // Filter by search text if provided
            if searchText.isEmpty {
                return true
            }
            
            if let message = log.message {
                return message.localizedCaseInsensitiveContains(searchText)
            }
            
            return false
        }
    }
    
    var body: some View {
        NavigationView {
            VStack {
                // Search field
                HStack {
                    Image(systemName: "magnifyingglass")
                        .foregroundColor(.secondary)
                    
                    TextField("Search logs", text: $searchText)
                        .textFieldStyle(RoundedBorderTextFieldStyle())
                    
                    if !searchText.isEmpty {
                        Button(action: { searchText = "" }) {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundColor(.secondary)
                        }
                    }
                }
                .padding(.horizontal)
                
                // Level filters
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack {
                        FilterToggle(
                            isOn: binding(for: .debug),
                            label: "Debug",
                            color: LogLevel.debug.color
                        )
                        
                        FilterToggle(
                            isOn: binding(for: .info),
                            label: "Info",
                            color: LogLevel.info.color
                        )
                        
                        FilterToggle(
                            isOn: binding(for: .warning),
                            label: "Warning",
                            color: LogLevel.warning.color
                        )
                        
                        FilterToggle(
                            isOn: binding(for: .error),
                            label: "Error",
                            color: LogLevel.error.color
                        )
                        
                        FilterToggle(
                            isOn: binding(for: .critical),
                            label: "Critical",
                            color: LogLevel.critical.color
                        )
                    }
                    .padding(.horizontal)
                }
                
                // Logs list
                if filteredLogs.isEmpty {
                    VStack {
                        Image(systemName: "doc.text.magnifyingglass")
                            .font(.system(size: 40))
                            .foregroundColor(.secondary)
                            .padding()
                        
                        Text("No logs found")
                            .foregroundColor(.secondary)
                        
                        if searchText.isEmpty && selectedLevels.isEmpty {
                            Button("Reset Filters") {
                                resetFilters()
                            }
                            .padding()
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    List {
                        ForEach(filteredLogs, id: \.timestampUs) { log in
                            LogRow(log: log)
                                .contentShape(Rectangle())
                                .onTapGesture {
                                    selectedLog = log
                                    showDetailSheet = true
                                }
                        }
                    }
                    .listStyle(PlainListStyle())
                }
            }
            .sheet(isPresented: $showDetailSheet) {
                if let log = selectedLog {
                    LogDetailView(log: log)
                }
            }
            .navigationTitle("Log Viewer")
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(action: refreshLogs) {
                        Image(systemName: "arrow.clockwise")
                    }
                }
                
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(action: exportLogs) {
                        Image(systemName: "square.and.arrow.up")
                    }
                }
            }
            .onAppear {
                refreshLogs()
            }
        }
    }
    
    private func binding(for level: LogLevel) -> Binding<Bool> {
        Binding<Bool>(
            get: { selectedLevels.contains(level) },
            set: { newValue in
                if newValue {
                    selectedLevels.insert(level)
                } else {
                    selectedLevels.remove(level)
                }
            }
        )
    }
    
    private func refreshLogs() {
        logs = loggerService.readAllLogs()
    }
    
    private func resetFilters() {
        selectedLevels = [.debug, .info, .warning, .error, .critical]
        searchText = ""
    }
    
    private func exportLogs() {
        // In a real app, we would export the logs to a file
        // and share it with UIActivityViewController
        refreshLogs()
    }
}

// Buffer Inspector View for detailed buffer visualization
struct BufferInspectorView: View {
    @EnvironmentObject var loggerService: LoggerService
    @State private var selectedSegment = 0
    
    var body: some View {
        NavigationView {
            VStack {
                Picker("Buffer", selection: $selectedSegment) {
                    Text("Memory").tag(0)
                    Text("Disk").tag(1)
                }
                .pickerStyle(SegmentedPickerStyle())
                .padding()
                
                if selectedSegment == 0 {
                    MemoryBufferInspector(stats: loggerService.bufferStats)
                } else {
                    DiskBufferInspector(stats: loggerService.bufferStats)
                }
            }
            .navigationTitle("Buffer Inspector")
        }
    }
}

// Memory Buffer Inspector
struct MemoryBufferInspector: View {
    let stats: BufferStats
    
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // Usage visualization
                VStack(alignment: .leading) {
                    Text("Memory Buffer Usage")
                        .font(.headline)
                    
                    HStack {
                        Text("\(Int(stats.memoryUsage))%")
                            .font(.title)
                            .bold()
                        
                        Spacer()
                        
                        VStack(alignment: .trailing) {
                            Text("Overflow Queue: \(stats.overflowQueueSize)")
                                .font(.caption)
                            
                            if let lastFlush = stats.lastFlushTime {
                                Text("Last Flush: \(lastFlush, formatter: itemFormatter)")
                                    .font(.caption)
                            }
                        }
                    }
                    
                    ProgressView(value: stats.memoryUsage, total: 100)
                        .progressViewStyle(LinearProgressViewStyle())
                }
                .padding()
                .background(Color(.systemBackground))
                .cornerRadius(10)
                .shadow(radius: 1)
                
                // Index positions
                VStack(alignment: .leading) {
                    Text("Buffer Indices")
                        .font(.headline)
                    
                    HStack {
                        VStack(alignment: .leading) {
                            Text("Write Index")
                                .font(.caption)
                            Text("\(stats.memoryWriteIndex)")
                                .font(.body)
                        }
                        
                        Spacer()
                        
                        VStack(alignment: .center) {
                            Text("Commit Index")
                                .font(.caption)
                            Text("\(stats.memoryCommitIndex)")
                                .font(.body)
                        }
                        
                        Spacer()
                        
                        VStack(alignment: .trailing) {
                            Text("Read Index")
                                .font(.caption)
                            Text("\(stats.memoryReadIndex)")
                                .font(.body)
                        }
                    }
                    
                    // Visual representation of indices
                    ZStack(alignment: .leading) {
                        // Buffer background
                        Rectangle()
                            .fill(Color(.systemGray5))
                            .frame(height: 20)
                            .cornerRadius(5)
                        
                        // Filled area
                        Rectangle()
                            .fill(Color.blue.opacity(0.5))
                            .frame(width: fillWidth(), height: 20)
                            .cornerRadius(5)
                        
                        // Write index marker
                        indexMarker(
                            position: Double(stats.memoryWriteIndex) / Double(bufferCapacity),
                            color: .blue
                        )
                        
                        // Commit index marker
                        indexMarker(
                            position: Double(stats.memoryCommitIndex) / Double(bufferCapacity),
                            color: .green
                        )
                        
                        // Read index marker
                        indexMarker(
                            position: Double(stats.memoryReadIndex) / Double(bufferCapacity),
                            color: .red
                        )
                    }
                }
                .padding()
                .background(Color(.systemBackground))
                .cornerRadius(10)
                .shadow(radius: 1)
                
                // Byte visualization
                VStack(alignment: .leading) {
                    Text("Memory Content Visualization")
                        .font(.headline)
                    
                    ScrollView(.horizontal, showsIndicators: true) {
                        byteVisualization()
                            .frame(height: 120)
                    }
                }
                .padding()
                .background(Color(.systemBackground))
                .cornerRadius(10)
                .shadow(radius: 1)
                
                // Details
                VStack(alignment: .leading) {
                    Text("Technical Details")
                        .font(.headline)
                    
                    DetailRow(label: "Buffer Size", value: "\(bufferCapacity) bytes")
                    DetailRow(label: "Available Space", value: "\(availableSpace()) bytes")
                    DetailRow(label: "Used Space", value: "\(usedSpace()) bytes")
                    DetailRow(label: "Overflow Queue Size", value: "\(stats.overflowQueueSize) records")
                }
                .padding()
                .background(Color(.systemBackground))
                .cornerRadius(10)
                .shadow(radius: 1)
            }
            .padding()
        }
        .background(Color(.systemGroupedBackground))
    }
    
    // Simulated buffer capacity for visualization
    private var bufferCapacity: UInt64 {
        return 1_048_576 // 1MB
    }
    
    private func fillWidth() -> CGFloat {
        let percentage = CGFloat(stats.memoryUsage) / 100.0
        return UIScreen.main.bounds.width - 40 * percentage
    }
    
    private func indexMarker(position: Double, color: Color) -> some View {
        ZStack {
            Triangle()
                .fill(color)
                .frame(width: 10, height: 10)
                .offset(y: -15)
            
            Circle()
                .fill(color)
                .frame(width: 10, height: 10)
        }
        .offset(x: (UIScreen.main.bounds.width - 40) * CGFloat(position) - 5)
    }
    
    private func byteVisualization() -> some View {
        // Generate a simulated byte view for demo purposes
        // In a real app, you'd visualize actual buffer contents
        
        let columns = 64
        let rows = 4
        let cellSize: CGFloat = 16
        
        return VStack(alignment: .leading, spacing: 2) {
            ForEach(0..<rows, id: \.self) { row in
                HStack(spacing: 2) {
                    // Address column
                    Text(String(format: "%08X", row * columns))
                        .font(.system(.caption, design: .monospaced))
                        .foregroundColor(.secondary)
                        .frame(width: 70, alignment: .leading)
                    
                    // Byte cells
                    ForEach(0..<columns, id: \.self) { col in
                        let index = row * columns + col
                        let value = simulatedByteValue(at: index)
                        
                        Text(value)
                            .font(.system(.caption, design: .monospaced))
                            .frame(width: cellSize, height: cellSize)
                            .background(cellBackground(for: index))
                            .cornerRadius(2)
                    }
                }
            }
        }
    }
    
    private func simulatedByteValue(at index: Int) -> String {
        // Generate simulated byte values
        // Different patterns for different regions (write, commit, read)
        
        let writePos = Int(stats.memoryWriteIndex) % Int(bufferCapacity)
        let commitPos = Int(stats.memoryCommitIndex) % Int(bufferCapacity)
        let readPos = Int(stats.memoryReadIndex) % Int(bufferCapacity)
        
        if index == writePos {
            return "W"
        } else if index == commitPos {
            return "C"
        } else if index == readPos {
            return "R"
        } else if index < readPos {
            // Already read area - empty
            return ".."
        } else if index < commitPos {
            // Committed but not read
            return String(format: "%02X", (index * 13) % 256)
        } else if index < writePos {
            // Written but not committed
            return "??"
        } else {
            // Unused area
            return ".."
        }
    }
    
    private func cellBackground(for index: Int) -> Color {
        let writePos = Int(stats.memoryWriteIndex) % Int(bufferCapacity)
        let commitPos = Int(stats.memoryCommitIndex) % Int(bufferCapacity)
        let readPos = Int(stats.memoryReadIndex) % Int(bufferCapacity)
        
        if index == writePos {
            return .blue
        } else if index == commitPos {
            return .green
        } else if index == readPos {
            return .red
        } else if index < readPos {
            return Color(.systemGray6)
        } else if index < commitPos {
            return Color.green.opacity(0.2)
        } else if index < writePos {
            return Color.blue.opacity(0.1)
        } else {
            return Color(.systemGray6)
        }
    }
    
    private func availableSpace() -> UInt64 {
        return bufferCapacity - usedSpace()
    }
    
    private func usedSpace() -> UInt64 {
        let readPos = stats.memoryReadIndex
        let writePos = stats.memoryWriteIndex
        
        if writePos >= readPos {
            return writePos - readPos
        } else {
            return bufferCapacity - readPos + writePos
        }
    }
}

// Disk Buffer Inspector
struct DiskBufferInspector: View {
    let stats: BufferStats
    
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // Usage visualization
                VStack(alignment: .leading) {
                    Text("Disk Buffer Usage")
                        .font(.headline)
                    
                    HStack {
                        Text("\(Int(stats.diskUsage))%")
                            .font(.title)
                            .bold()
                        
                        Spacer()
                        
                        VStack(alignment: .trailing) {
                            Text("Wrap Count: \(stats.wrapCount)")
                                .font(.caption)
                            
                            if let lastFlush = stats.lastFlushTime {
                                Text("Last Flush: \(lastFlush, formatter: itemFormatter)")
                                    .font(.caption)
                            }
                        }
                    }
                    
                    ProgressView(value: stats.diskUsage, total: 100)
                        .progressViewStyle(LinearProgressViewStyle())
                }
                .padding()
                .background(Color(.systemBackground))
                .cornerRadius(10)
                .shadow(radius: 1)
                
                // Position indicators
                VStack(alignment: .leading) {
                    Text("Buffer Positions")
                        .font(.headline)
                    
                    HStack {
                        VStack(alignment: .leading) {
                            Text("Write Position")
                                .font(.caption)
                            Text("\(stats.diskWritePos)")
                                .font(.body)
                        }
                        
                        Spacer()
                        
                        VStack(alignment: .trailing) {
                            Text("Read Position")
                                .font(.caption)
                            Text("\(stats.diskReadPos)")
                                .font(.body)
                        }
                    }
                    
                    // Visual representation of positions
                    ZStack(alignment: .leading) {
                        // Buffer background
                        Rectangle()
                            .fill(Color(.systemGray5))
                            .frame(height: 20)
                            .cornerRadius(5)
                        
                        // Filled area
                        Rectangle()
                            .fill(Color.green.opacity(0.5))
                            .frame(width: diskFillWidth(), height: 20)
                            .cornerRadius(5)
                        
                        // Write position marker
                        indexMarker(
                            position: Double(stats.diskWritePos) / Double(diskBufferCapacity),
                            color: .green
                        )
                        
                        // Read position marker
                        indexMarker(
                            position: Double(stats.diskReadPos) / Double(diskBufferCapacity),
                            color: .red
                        )
                        
                        // Wrap indicator
                        if stats.wrapCount > 0 {
                            Text("↻")
                                .font(.title2)
                                .foregroundColor(.orange)
                                .background(Circle().fill(Color.white).frame(width: 20, height: 20))
                                .offset(x: UIScreen.main.bounds.width - 60, y: 0)
                        }
                    }
                }
                .padding()
                .background(Color(.systemBackground))
                .cornerRadius(10)
                .shadow(radius: 1)
                
                // File structure
                VStack(alignment: .leading) {
                    Text("File Structure")
                        .font(.headline)
                    
                    VStack(alignment: .leading, spacing: 10) {
                        // Control block
                        HStack {
                            RoundedRectangle(cornerRadius: 4)
                                .fill(Color.purple.opacity(0.2))
                                .frame(width: 80, height: 30)
                                .overlay(
                                    Text("Control")
                                        .font(.caption)
                                )
                            
                            Text("Header data, CRC32, indices")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                        
                        // Records visualization
                        HStack(spacing: 2) {
                            ForEach(0..<10, id: \.self) { i in
                                let isFilled = isRecordFilled(index: i)
                                let color: Color = isFilled ? .green : Color(.systemGray5)
                                
                                VStack(spacing: 0) {
                                    // Record header
                                    RoundedRectangle(cornerRadius: 2)
                                        .fill(color.opacity(0.8))
                                        .frame(width: 30, height: 10)
                                    
                                    // Record data
                                    RoundedRectangle(cornerRadius: 2)
                                        .fill(color.opacity(0.4))
                                        .frame(width: 30, height: 30)
                                }
                            }
                        }
                    }
                }
                .padding()
                .background(Color(.systemBackground))
                .cornerRadius(10)
                .shadow(radius: 1)
                
                // Technical details
                VStack(alignment: .leading) {
                    Text("Technical Details")
                        .font(.headline)
                    
                    DetailRow(label: "Buffer Size", value: "\(diskBufferCapacity) bytes")
                    DetailRow(label: "Available Space", value: "\(diskAvailableSpace()) bytes")
                    DetailRow(label: "Used Space", value: "\(diskUsedSpace()) bytes")
                    DetailRow(label: "Wrap Count", value: "\(stats.wrapCount)")
                    DetailRow(label: "Control Block Size", value: "64 bytes")
                }
                .padding()
                .background(Color(.systemBackground))
                .cornerRadius(10)
                .shadow(radius: 1)
            }
            .padding()
        }
        .background(Color(.systemGroupedBackground))
    }
    
    // Simulated disk buffer capacity for visualization
    private var diskBufferCapacity: UInt64 {
        return 10_485_760 // 10MB
    }
    
    private func diskFillWidth() -> CGFloat {
        let percentage = CGFloat(stats.diskUsage) / 100.0
        return (UIScreen.main.bounds.width - 40) * percentage
    }
    
    private func indexMarker(position: Double, color: Color) -> some View {
        ZStack {
            Triangle()
                .fill(color)
                .frame(width: 10, height: 10)
                .offset(y: -15)
            
            Circle()
                .fill(color)
                .frame(width: 10, height: 10)
        }
        .offset(x: (UIScreen.main.bounds.width - 40) * CGFloat(position) - 5)
    }
    
    private func isRecordFilled(index: Int) -> Bool {
        let readPos = Int(stats.diskReadPos) % Int(diskBufferCapacity)
        let writePos = Int(stats.diskWritePos) % Int(diskBufferCapacity)
        
        // Simplified visualization - in reality, this would be based on actual record positions
        let recordSize = Int(diskBufferCapacity) / 20
        let recordIndex = index * recordSize
        
        if writePos >= readPos {
            return recordIndex >= readPos && recordIndex < writePos
        } else {
            return recordIndex >= readPos || recordIndex < writePos
        }
    }
    
    private func diskAvailableSpace() -> UInt64 {
        return diskBufferCapacity - diskUsedSpace()
    }
    
    private func diskUsedSpace() -> UInt64 {
        let readPos = stats.diskReadPos
        let writePos = stats.diskWritePos
        
        if writePos >= readPos {
            return writePos - readPos
        } else {
            return diskBufferCapacity - readPos + writePos
        }
    }
}

// Settings View
struct SettingsView: View {
    @EnvironmentObject var loggerService: LoggerService
    @State private var volatileBufferSize: Double = 1
    @State private var persistentBufferSize: Double = 10
    @State private var flushIntervalMs: Double = 1000
    @State private var highWatermarkPercent: Double = 75
    @State private var showResetAlert = false
    
    var body: some View {
        NavigationView {
            Form {
                Section(header: Text("Buffer Configuration")) {
                    VStack(alignment: .leading) {
                        Text("Memory Buffer Size: \(Int(volatileBufferSize)) MB")
                        Slider(value: $volatileBufferSize, in: 0.25...10, step: 0.25)
                    }
                    
                    VStack(alignment: .leading) {
                        Text("Disk Buffer Size: \(Int(persistentBufferSize)) MB")
                        Slider(value: $persistentBufferSize, in: 1...100, step: 1)
                    }
                }
                
                Section(header: Text("Flush Configuration")) {
                    VStack(alignment: .leading) {
                        Text("Flush Interval: \(Int(flushIntervalMs)) ms")
                        Slider(value: $flushIntervalMs, in: 100...5000, step: 100)
                    }
                    
                    VStack(alignment: .leading) {
                        Text("High Watermark: \(Int(highWatermarkPercent))%")
                        Slider(value: $highWatermarkPercent, in: 10...95, step: 5)
                    }
                }
                
                Section {
                    Button("Apply Configuration") {
                        applyConfiguration()
                    }
                    .buttonStyle(BorderedProminentButtonStyle())
                    
                    Button("Reset to Defaults") {
                        resetToDefaults()
                    }
                }
                
                Section(header: Text("Danger Zone")) {
                    Button("Reset All Data") {
                        showResetAlert = true
                    }
                    .foregroundColor(.red)
                    .alert(isPresented: $showResetAlert) {
                        Alert(
                            title: Text("Reset All Data"),
                            message: Text("This will clear all logs and reset both buffers. This action cannot be undone."),
                            primaryButton: .destructive(Text("Reset")) {
                                resetAllData()
                            },
                            secondaryButton: .cancel()
                        )
                    }
                }
                
                Section(header: Text("About")) {
                    VStack(alignment: .leading, spacing: 10) {
                        Text("Sherlog Ring Buffer")
                            .font(.headline)
                        
                        Text("A high-performance, lock-free ring buffer implementation for mobile application logging.")
                            .font(.caption)
                            .foregroundColor(.secondary)
                        
                        Divider()
                        
                        Text("Version: 0.1.0")
                            .font(.caption)
                        
                        Text("License: MIT")
                            .font(.caption)
                    }
                    .padding(.vertical, 8)
                }
            }
            .navigationTitle("Settings")
        }
    }
    
    private func applyConfiguration() {
        // Convert MB to bytes
        let memorySize = UInt32(volatileBufferSize * 1024 * 1024)
        let diskSize = UInt32(persistentBufferSize * 1024 * 1024)
        
        loggerService.updateConfiguration(
            memorySize: memorySize,
            diskSize: diskSize,
            flushMs: UInt32(flushIntervalMs),
            watermark: Float(highWatermarkPercent)
        )
    }
    
    private func resetToDefaults() {
        volatileBufferSize = 1
        persistentBufferSize = 10
        flushIntervalMs = 1000
        highWatermarkPercent = 75
        
        applyConfiguration()
    }
    
    private func resetAllData() {
        loggerService.clearMemoryBuffer()
        loggerService.resetDiskBuffer()
    }
}

// MARK: - Reusable Components

// Status card component
struct StatusCard: View {
    let title: String
    let value: String
    let systemImage: String
    let color: Color
    
    var body: some View {
        VStack(alignment: .leading) {
            HStack {
                Image(systemName: systemImage)
                    .font(.title2)
                    .foregroundColor(color)
                
                Text(title)
                    .font(.headline)
            }
            
            Text(value)
                .font(.title)
                .bold()
                .padding(.top, 2)
        }
        .padding()
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(.systemBackground))
        .cornerRadius(10)
        .shadow(radius: 2)
    }
}

// Buffer visualization component
struct BufferVisualization: View {
    let title: String
    let percentage: Float
    let color: Color
    
    var body: some View {
        VStack(alignment: .leading) {
            Text(title)
                .font(.headline)
            
            HStack {
                Text("\(Int(percentage))%")
                    .font(.subheadline)
                    .bold()
                
                Spacer()
            }
            
            ZStack(alignment: .leading) {
                Rectangle()
                    .fill(Color(.systemGray5))
                    .frame(height: 20)
                    .cornerRadius(5)
                
                Rectangle()
                    .fill(color)
                    .frame(width: fillWidth(), height: 20)
                    .cornerRadius(5)
            }
        }
    }
    
    private func fillWidth() -> CGFloat {
        let percentage = CGFloat(percentage) / 100.0
        return UIScreen.main.bounds.width * 0.8 * percentage
    }
}

// Action button component
struct ActionButton: View {
    let title: String
    let systemImage: String
    let action: () -> Void
    
    var body: some View {
        Button(action: action) {
            VStack {
                Image(systemName: systemImage)
                    .font(.title2)
                
                Text(title)
                    .font(.caption)
            }
            .frame(maxWidth: .infinity)
            .padding()
            .background(Color(.systemGray6))
            .cornerRadius(10)
        }
        .buttonStyle(PlainButtonStyle())
    }
}

// Log row component
struct LogRow: View {
    let log: LogRecord
    
    var logLevel: LogLevel {
        LogLevel(rawValue: log.tag) ?? .info
    }
    
    var message: String {
        log.message ?? "[Binary data]"
    }
    
    var timestamp: Date {
        Date(timeIntervalSince1970: TimeInterval(log.timestampUs) / 1_000_000)
    }
    
    var body: some View {
        HStack(alignment: .top) {
            Circle()
                .fill(logLevel.color)
                .frame(width: 10, height: 10)
                .padding(.top, 5)
            
            VStack(alignment: .leading, spacing: 4) {
                Text(timestamp, formatter: itemFormatter)
                    .font(.caption)
                    .foregroundColor(.secondary)
                
                Text(message)
                    .font(.body)
                    .lineLimit(2)
            }
        }
        .padding(.vertical, 4)
    }
}

// Log detail view
struct LogDetailView: View {
    let log: LogRecord
    @Environment(\.dismiss) private var dismiss
    
    var logLevel: LogLevel {
        LogLevel(rawValue: log.tag) ?? .info
    }
    
    var message: String {
        log.message ?? "[Binary data]"
    }
    
    var timestamp: Date {
        Date(timeIntervalSince1970: TimeInterval(log.timestampUs) / 1_000_000)
    }
    
    var metadata: [String: String] {
        // Extract metadata from message if it contains the metadata format
        // This is a simplified implementation assuming metadata is in [Metadata: key: value, key2: value2] format
        if let metadataStart = message.range(of: "[Metadata: "),
           let metadataEnd = message.range(of: "]", range: metadataStart.upperBound..<message.endIndex) {
            let metadataString = message[metadataStart.upperBound..<metadataEnd.lowerBound]
            
            var result: [String: String] = [:]
            let pairs = metadataString.components(separatedBy: ", ")
            
            for pair in pairs {
                let components = pair.components(separatedBy: ": ")
                if components.count == 2 {
                    result[components[0]] = components[1]
                }
            }
            
            return result
        }
        
        return [:]
    }
    
    var cleanMessage: String {
        if let metadataStart = message.range(of: " [Metadata: ") {
            return String(message[..<metadataStart.lowerBound])
        }
        return message
    }
    
    var body: some View {
        NavigationView {
            Form {
                Section(header: Text("Log Details")) {
                    HStack {
                        Text("Level")
                            .bold()
                        Spacer()
                        Text(logLevel.name)
                            .foregroundColor(logLevel.color)
                    }
                    
                    HStack {
                        Text("Timestamp")
                            .bold()
                        Spacer()
                        Text(timestamp, formatter: itemFormatter)
                    }
                    
                    HStack {
                        Text("CRC32")
                            .bold()
                        Spacer()
                        Text("\(log.crc32)")
                            .font(.system(.body, design: .monospaced))
                    }
                }
                
                Section(header: Text("Message")) {
                    Text(cleanMessage)
                        .font(.body)
                }
                
                if !metadata.isEmpty {
                    Section(header: Text("Metadata")) {
                        ForEach(metadata.sorted(by: { $0.key < $1.key }), id: \.key) { key, value in
                            HStack {
                                Text(key)
                                    .bold()
                                Spacer()
                                Text(value)
                            }
                        }
                    }
                }
                
                Section(header: Text("Raw Data")) {
                    HStack {
                        Text("Size")
                            .bold()
                        Spacer()
                        Text("\(log.data.count) bytes")
                    }
                    
                    if let hexData = hexString(from: log.data, maxLength: 100) {
                        Text(hexData)
                            .font(.system(.caption, design: .monospaced))
                            .lineLimit(nil)
                    }
                }
                
                Button("Close") {
                    dismiss()
                }
                .buttonStyle(BorderedProminentButtonStyle())
            }
            .navigationTitle("Log Detail")
            .navigationBarTitleDisplayMode(.inline)
        }
    }
    
    private func hexString(from data: Data, maxLength: Int) -> String? {
        let displayData = data.count > maxLength ? data.prefix(maxLength) : data
        let hexString = displayData.map { String(format: "%02X", $0) }.joined(separator: " ")
        
        if data.count > maxLength {
            return hexString + "... (\(data.count - maxLength) more bytes)"
        }
        
        return hexString
    }
}

// Filter toggle component
struct FilterToggle: View {
    @Binding var isOn: Bool
    let label: String
    let color: Color
    
    var body: some View {
        Button(action: { isOn.toggle() }) {
            HStack {
                Circle()
                    .fill(color)
                    .frame(width: 10, height: 10)
                
                Text(label)
                    .font(.caption)
            }
            .padding(.vertical, 8)
            .padding(.horizontal, 12)
            .background(isOn ? color.opacity(0.2) : Color(.systemGray6))
            .cornerRadius(20)
            .overlay(
                RoundedRectangle(cornerRadius: 20)
                    .stroke(color, lineWidth: isOn ? 1 : 0)
            )
        }
        .buttonStyle(PlainButtonStyle())
    }
}

// Detail row component
struct DetailRow: View {
    let label: String
    let value: String
    
    var body: some View {
        HStack {
            Text(label)
                .foregroundColor(.secondary)
            Spacer()
            Text(value)
                .bold()
        }
        .padding(.vertical, 4)
    }
}

// Triangle shape for markers
struct Triangle: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.midX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}

// MARK: - Utilities

// Date formatter
private let itemFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.dateStyle = .short
    formatter.timeStyle = .medium
    return formatter
}()

// Preview
struct ContentView_Previews: PreviewProvider {
    static var previews: some View {
        ContentView()
            .environmentObject(LoggerService())
    }
}