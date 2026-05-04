# EdgeFabric Quantum UI Integration Plan

## Overview
Integrate a comprehensive Web UI for Quantum computing in EdgeFabric, allowing users to:
- Create and configure Quantum Accelerators/Backends
- Design quantum circuits (graphical + code)
- Submit and monitor quantum jobs
- Visualize results and histograms
- Manage cloud quantum backends (IBM Quantum, AWS Braket, Azure Quantum, Google Quantum)

## Architecture

### Frontend (ef-dashboard) - Leptos WASM
**New Pages:**
1. **Quantum Overview** (`pages/quantum.rs`)
   - Dashboard with backend status
   - Recent jobs and jobs queue
   - Quick stats (total accelerators, active jobs, etc.)

2. **Quantum Accelerators** (`pages/quantum_accelerators.rs`)
   - List all configured accelerators (local + cloud)
   - Create/Edit/Delete accelerators
   - Backend configuration panels
   - Simulated vs. Real backend indicators

3. **Circuit Designer** (`pages/quantum_designer.rs`)
   - Dual-mode interface:
     - **Visual Designer**: Drag-drop gates, qubit visualization
     - **Code Editor**: OpenQASM/Qiskit Python input
   - Real-time circuit preview
   - Circuit syntax validation
   - Save/Load circuit templates
   - Export to OpenQASM/Qiskit

4. **Quantum Jobs** (`pages/quantum_jobs.rs`)
   - Job submission UI
   - Backend selection
   - Shot configuration
   - Job queue monitoring
   - Real-time status updates (WebSocket)

5. **Job Results & Visualization** (`pages/quantum_results.rs`)
   - Full histogram display (bar charts)
   - Bitstring distribution analysis
   - Expectation value calculations
   - Export results (CSV, JSON, PNG)
   - Compare multiple job results

### Components (ef-dashboard/src/components)
1. **CircuitCanvas** (`circuit_canvas.rs`)
   - Interactive canvas for circuit visualization
   - Gate rendering with SVG
   - Drag-drop gate placement
   - Qubit/classical bit management

2. **GateSelector** (`gate_selector.rs`)
   - Gate palette (single-qubit, two-qubit, measurements, barriers)
   - Gate parameter controls (rotation angles)
   - Drag-drop gateway

3. **BackendConfig** (`backend_config.rs`)
   - IBM Quantum form (API token, instance, backend)
   - AWS Braket form (credentials, device ARN)
   - Azure Quantum form (workspace, subscription)
   - Local simulator configuration

4. **JobMonitor** (`job_monitor.rs`)
   - Real-time job status polling/WebSocket
   - Progress indicators
   - Log streaming
   - Cancel job button

5. **HistogramViewer** (`histogram_viewer.rs`)
   - Bar chart with Plotly.js or similar
   - Interactive hover labels
   - Statistics panel (mean, std dev, entropy)
   - Bitstring filtering

6. **CircuitPreview** (`circuit_preview.rs`)
   - SVG-based circuit rendering
   - Scrollable for large circuits
   - Qubit count and gate count display

### Backend (ef-api) - New Routes

**REST Endpoints:**

```
POST /api/v1/quantum/accelerators
  Create new accelerator/backend config
  Body: {
    name: string,
    backend_type: "simulator" | "ibm" | "braket" | "azure" | "google",
    config: { ... } // backend-specific config
  }

GET /api/v1/quantum/accelerators
  List all accelerators

GET /api/v1/quantum/accelerators/:id
  Get accelerator details

PUT /api/v1/quantum/accelerators/:id
  Update accelerator config

DELETE /api/v1/quantum/accelerators/:id
  Delete accelerator

POST /api/v1/quantum/circuits
  Save circuit template
  Body: {
    name: string,
    description: string,
    qasm: string,
    metadata: {...}
  }

GET /api/v1/quantum/circuits
  List saved circuits

GET /api/v1/quantum/circuits/:id
  Get circuit details

POST /api/v1/quantum/jobs
  Submit job
  Body: {
    accelerator_id: string,
    circuit_id?: string,
    qasm?: string,
    shots: number,
    timeout_sec: number
  }

GET /api/v1/quantum/jobs/:id
  Get job details + status

GET /api/v1/quantum/jobs/:id/results
  Get job results (histogram, bitstrings)

WebSocket /ws/quantum/jobs/:id
  Subscribe to real-time job status
```

## Data Models

### Accelerator
```rust
struct QuantumAccelerator {
    id: String,
    name: String,
    backend_type: BackendType,
    config: BackendConfig,
    max_qubits: u16,
    status: AcceleratorStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

enum BackendType {
    Simulator,
    IbmQuantum,
    AwsBraket,
    AzureQuantum,
    GoogleQuantum,
}

enum AcceleratorStatus {
    Available,
    Busy,
    Offline,
    Error(String),
}
```

### QuantumJob
```rust
struct QuantumJob {
    id: String,
    accelerator_id: String,
    circuit_id: Option<String>,
    qasm: String,
    shots: u32,
    status: JobStatus,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    remote_job_id: Option<String>, // For cloud backends
    results: Option<JobResults>,
    error: Option<String>,
}

enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

struct JobResults {
    histogram: HashMap<String, u32>, // bitstring -> count
    top_bitstring: String,
    total_shots: u32,
    execution_time_ms: u64,
}
```

### Circuit
```rust
struct QuantumCircuit {
    id: String,
    name: String,
    description: String,
    qasm: String,
    qubit_count: u16,
    gate_count: u16,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

## UI/UX Design

### Navigation
Add to sidebar:
```
Quantum (new section)
├── Overview
├── Accelerators
├── Circuit Designer
├── Jobs
└── Results
```

### Color Scheme & Icons
- Quantum icon: Atom/circuit symbol 🔬⚛️
- Accelerator status: Green (Available), Orange (Busy), Red (Offline)
- Gate types: Different colors (Clifford, Rotation, Measurement, Barrier)

### Key Interactions

**Circuit Designer Workflow:**
1. Create new circuit (2+ qubits default)
2. Drag gates onto qubits or type OpenQASM
3. Real-time preview updates
4. Save circuit or submit immediately
5. Choose backend/accelerator
6. Configure shots and options
7. Submit job

**Job Monitoring:**
1. Submit job → auto-redirect to job details
2. Real-time status updates
3. Download results when complete
4. View histogram visualization

## Implementation Phases

### Phase 1: Foundation (Week 1-2)
- [ ] Create quantum.rs overview page
- [ ] Create quantum_accelerators.rs CRUD pages
- [ ] Add backend config components
- [ ] Wire API endpoints (simulated responses for now)
- [ ] Add sidebar navigation

### Phase 2: Circuit Designer (Week 2-3)
- [ ] Implement CircuitCanvas component
- [ ] Add GateSelector palette
- [ ] Create circuit_designer.rs page
- [ ] OpenQASM parser/renderer
- [ ] Save/Load circuits
- [ ] Export functionality

### Phase 3: Job Management (Week 3-4)
- [ ] Create quantum_jobs.rs page
- [ ] Implement JobMonitor component
- [ ] Wire job submission API
- [ ] Real-time polling (later: WebSocket)
- [ ] Job history/list view

### Phase 4: Results & Visualization (Week 4-5)
- [ ] Create quantum_results.rs page
- [ ] Implement HistogramViewer with charting
- [ ] Results analysis (statistics, export)
- [ ] Result comparison UI
- [ ] Integration with VeerOS backend

### Phase 5: Integration (Week 5-6)
- [ ] Connect to VeerOS Quantum APIs
- [ ] Test with real simulators
- [ ] Cloud backend integration (IBM, AWS, Azure)
- [ ] WebSocket real-time updates
- [ ] Performance optimization

## VeerOS Integration Points

The Quantum UI will integrate with VeerOS crates:

1. **quantum crate** (Circuit IR, Backends, Simulators)
   - Use Circuit struct from quantum crate
   - Support all gate types and operations

2. **veer_cli** (QASM parsing, CLI tool)
   - Leverage existing QASM parser
   - Reuse endpoint management
   - Share simulation engine

3. **microkernel** (Syscall handlers)
   - SYS_QPU_SUBMIT for local execution
   - QuantumCircuitTable for handle management
   - Accelerator registry for device discovery

4. **IBM Quantum Bridge**
   - Direct connection to ibm.rs backend
   - Real cloud job submission
   - Histogram extraction and caching

## Database Schema Extensions

```sql
-- Quantum Accelerators
CREATE TABLE quantum_accelerators (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL,
  name VARCHAR(255) NOT NULL,
  backend_type VARCHAR(50) NOT NULL,
  config JSONB NOT NULL,
  max_qubits INT NOT NULL,
  status VARCHAR(50) NOT NULL,
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW(),
  FOREIGN KEY (tenant_id) REFERENCES tenants(id)
);

-- Quantum Circuits
CREATE TABLE quantum_circuits (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL,
  name VARCHAR(255) NOT NULL,
  description TEXT,
  qasm TEXT NOT NULL,
  qubit_count INT NOT NULL,
  gate_count INT NOT NULL,
  metadata JSONB,
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW(),
  FOREIGN KEY (tenant_id) REFERENCES tenants(id)
);

-- Quantum Jobs
CREATE TABLE quantum_jobs (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL,
  accelerator_id UUID NOT NULL,
  circuit_id UUID,
  qasm TEXT NOT NULL,
  shots INT NOT NULL,
  status VARCHAR(50) NOT NULL,
  remote_job_id VARCHAR(255),
  results JSONB,
  error TEXT,
  created_at TIMESTAMP DEFAULT NOW(),
  started_at TIMESTAMP,
  completed_at TIMESTAMP,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id),
  FOREIGN KEY (accelerator_id) REFERENCES quantum_accelerators(id),
  FOREIGN KEY (circuit_id) REFERENCES quantum_circuits(id)
);

-- Quantum Job History/Audit
CREATE TABLE quantum_job_events (
  id UUID PRIMARY KEY,
  job_id UUID NOT NULL,
  event_type VARCHAR(50) NOT NULL,
  message TEXT,
  created_at TIMESTAMP DEFAULT NOW(),
  FOREIGN KEY (job_id) REFERENCES quantum_jobs(id)
);
```

## Dependencies to Add

### ef-dashboard (Cargo.toml additions)
```toml
# Circuit visualization
plotly = "0.8"
d3-delaunay = "0.4"

# OpenQASM parsing (optional, for full spec support)
# qiskit-rs = { version = "0.1", optional = true }

# WebSocket for real-time updates
tokio-tungstenite = "0.20"

# Additional utilities
uuid = { version = "1", features = ["v4", "serde"] }
```

### ef-api (Cargo.toml additions)
```toml
# Database models
sqlx = { version = "0.7", features = ["runtime-tokio-native-tls", "postgres", "uuid", "json", "chrono"] }

# VeerOS integration
quantum = { path = "../../VeerOS/crates/quantum" }
veer_cli = { path = "../../VeerOS/crates/veer_cli" }

# WebSocket support
tokio-tungstenite = "0.20"
futures = "0.3"
```

## Security Considerations

1. **API Token Storage** (IBM, AWS, Azure)
   - Encrypt credentials in DB
   - Use environment variables for sensitive data
   - Rotate tokens periodically
   - Audit access logs

2. **QASM Validation**
   - Sanitize uploaded circuits
   - Limit circuit complexity (gates, qubits)
   - Whitelist allowed operations
   - Timeout for malicious circuits

3. **Resource Limits**
   - Max qubits per job
   - Max shots per submission
   - Rate limiting per tenant
   - Cost estimation & approval workflow

4. **Authorization**
   - Tenant isolation
   - Backend access control
   - Job ownership verification
   - Result access restrictions

## Success Metrics

- Users can submit quantum jobs in <5 clicks
- Circuit designer supports all common gates
- Job status updates in <2 seconds
- Histogram visualization loads in <1 second
- Support for 3+ cloud providers in Phase 1
- 99% API uptime for Quantum endpoints

## Risks & Mitigation

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Cloud API quota limits | Job failures | Implement queue, cost tracking, approval workflow |
| Network latency | Slow updates | WebSocket, polling strategies, caching |
| Complex QASM parsing | Security issues | Use proven libraries, validation, sandboxing |
| User mistakes (loops, etc.) | Failed jobs | UI validation, circuit limits, clear errors |
| Cost overruns | Budget issues | Quotas, cost estimation, approval required |

---

**Next Steps:**
1. Review and approve this plan
2. Set up database schema
3. Start Phase 1 implementation
4. Create GitHub issues for tracking
5. Set up CI/CD for Quantum UI tests
