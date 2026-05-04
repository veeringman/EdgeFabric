use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;

use ef_core::config::VeerOsConfig;
use ef_core::error::{EdgeFabricError, Result};
use quantum::{Circuit, QuantumBackend};
use quantum::simulator::SimulatorBackend;
use serde_json::{Value, json};

use super::CommandOutput;

#[derive(Debug, Clone)]
pub struct VeerOsShellInfo {
    pub vm_name: String,
    pub host: String,
    pub port: u16,
    pub connect_command: String,
}

#[derive(Debug, Clone, Copy)]
pub enum QuantumProgram {
    Bell,
    Ghz3,
    Qft3,
}

impl QuantumProgram {
    pub fn as_cli_value(self) -> &'static str {
        match self {
            Self::Bell => "bell",
            Self::Ghz3 => "ghz3",
            Self::Qft3 => "qft3",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum QuantumEndpointVariant {
    Default,
    Staging,
}

impl QuantumEndpointVariant {
    pub fn as_cli_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Staging => "staging",
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuantumSimulationRequest {
    pub program: Option<QuantumProgram>,
    pub qasm: Option<String>,
    pub shots: u32,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct QuantumIbmRunRequest {
    pub program: Option<QuantumProgram>,
    pub qasm_file: Option<String>,
    pub shots: u32,
    pub backend: String,
    pub token: Option<String>,
    pub instance: Option<String>,
    pub poll_ms: u64,
    pub timeout_sec: u64,
    pub histogram: bool,
    pub endpoint: Option<QuantumEndpointVariant>,
    pub json: bool,
}

pub struct VeerOsDriver;

impl VeerOsDriver {
    pub fn sanitize_vm_name(name: &str, suffix: &str) -> String {
        let mut out = String::with_capacity(name.len() + suffix.len() + 1);
        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
            } else if (ch == '-' || ch == '_' || ch == ' ') && !out.ends_with('-') {
                out.push('-');
            }
        }
        while out.ends_with('-') {
            out.pop();
        }
        if out.is_empty() {
            out.push_str("virtual-iot");
        }
        out.push('-');
        out.push_str(suffix);
        out
    }

    pub async fn create_vm(
        cfg: &VeerOsConfig,
        vm_name: &str,
        hostname: &str,
        target: &str,
        memory_mb: u32,
        net_mode: &str,
    ) -> Result<CommandOutput> {
        Self::run_vm_command(
            cfg,
            &[
                "create",
                vm_name,
                "--target",
                target,
                "--memory",
                &memory_mb.to_string(),
                "--net",
                net_mode,
                "--hostname",
                hostname,
            ],
        )
        .await
    }

    pub async fn start_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        Self::run_vm_command(cfg, &["start", vm_name]).await
    }

    pub async fn stop_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        Self::run_vm_command(cfg, &["stop", vm_name]).await
    }

    pub async fn restart_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        Self::run_vm_command(cfg, &["restart", vm_name]).await
    }

    pub async fn destroy_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        Self::run_vm_command(cfg, &["destroy", vm_name]).await
    }

    pub async fn vm_config(cfg: &VeerOsConfig, vm_name: &str) -> Result<HashMap<String, String>> {
        let output = Self::run_vm_command(cfg, &["config", vm_name]).await?;
        if !output.success() {
            return Err(EdgeFabricError::Internal(format!(
                "veeros-vm config failed: {}",
                output.stderr
            )));
        }
        let mut config = HashMap::new();
        for line in output.stdout.lines() {
            if line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                config.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
        Ok(config)
    }

    pub async fn vm_status(cfg: &VeerOsConfig, vm_name: &str) -> Result<String> {
        let output = Self::run_vm_command(cfg, &["status", vm_name]).await?;
        if !output.success() {
            return Err(EdgeFabricError::Internal(format!(
                "veeros-vm status failed: {}",
                output.stderr
            )));
        }
        let line = output.stdout.lines().next().unwrap_or_default().to_ascii_lowercase();
        if line.contains("running") {
            Ok("running".into())
        } else {
            Ok("stopped".into())
        }
    }

    pub async fn list_vms(cfg: &VeerOsConfig) -> Result<Vec<serde_json::Value>> {
        let output = Self::run_vm_command(cfg, &["list-json"]).await?;
        if !output.success() {
            return Err(EdgeFabricError::Internal(format!(
                "veeros-vm list-json failed: {}",
                output.stderr
            )));
        }
        let vms: Vec<serde_json::Value> = serde_json::from_str(output.stdout.trim())
            .map_err(|e| EdgeFabricError::Internal(format!("failed to parse VM list: {e}")))?;
        Ok(vms)
    }

    pub async fn sensor_inject(
        cfg: &VeerOsConfig,
        vm_name: &str,
        sensor: &str,
        value: &str,
    ) -> Result<CommandOutput> {
        Self::run_vm_command(cfg, &["sensor", "inject", vm_name, sensor, value]).await
    }

    pub async fn shell_info(cfg: &VeerOsConfig, vm_name: &str) -> Result<VeerOsShellInfo> {
        let config = Self::vm_config(cfg, vm_name).await?;
        let port = config
            .get("ssh_port")
            .and_then(|v| v.parse::<u16>().ok())
            .ok_or_else(|| EdgeFabricError::Internal("VeerOS VM is missing ssh_port".into()))?;
        let host = "localhost".to_string();
        Ok(VeerOsShellInfo {
            vm_name: vm_name.to_string(),
            host: host.clone(),
            port,
            connect_command: format!("{} shell {} {}", cfg.veer_connect_path, host, port),
        })
    }

    pub fn quantum_backend_catalog(cfg: &VeerOsConfig) -> Vec<Value> {
        let sim = SimulatorBackend::new();
        let sim_info = sim.backend_info();

        vec![
            json!({
                "id": "veeros-local-sim",
                "provider": "veeros",
                "kind": "simulator",
                "name": sim_info.name,
                "max_qubits": sim_info.max_qubits,
                "queue_depth": sim_info.queue_depth,
                "supports": {
                    "program_templates": ["bell", "ghz3", "qft3"],
                    "qasm": true,
                    "histogram": true
                }
            }),
            json!({
                "id": "ibm-quantum-cloud",
                "provider": "ibm-quantum",
                "kind": "cloud",
                "name": "IBM Quantum",
                "access": {
                    "via_cli": cfg.veer_cli_path,
                    "cli_available": Path::new(&cfg.veer_cli_path).exists(),
                    "endpoint_variants": ["default", "staging"]
                },
                "supports": {
                    "program_templates": ["bell", "ghz3", "qft3"],
                    "qasm_file": true,
                    "histogram": true
                },
                "required_credentials": ["token", "instance", "backend"]
            }),
        ]
    }

    pub fn simulate_quantum(req: &QuantumSimulationRequest) -> Result<Value> {
        let circuit = Self::build_circuit(req.program, req.qasm.as_deref())?;
        circuit
            .status()
            .map_err(|e| EdgeFabricError::Internal(format!("invalid quantum circuit: {:?}", e)))?;

        let mut backend = if let Some(seed) = req.seed {
            SimulatorBackend::with_seed(seed)
        } else {
            SimulatorBackend::new()
        };

        let hist = backend
            .execute_shots(&circuit, req.shots)
            .map_err(|e| EdgeFabricError::Internal(format!("simulation failed: {:?}", e)))?;

        let qubits = circuit.logical_qubits as usize;
        let mut histogram = serde_json::Map::new();
        let mut top_bitstring = String::new();
        let mut top_count = 0u32;

        let mut slot = 0usize;
        while let Some((key, count)) = hist.bin(slot) {
            let bitstring = Self::histogram_key_to_bitstring(key, qubits);
            if count > top_count {
                top_count = count;
                top_bitstring = bitstring.clone();
            }
            histogram.insert(bitstring, json!(count));
            slot += 1;
        }

        Ok(json!({
            "provider": "veeros",
            "backend": "veeros-local-sim",
            "shots": req.shots,
            "top_bitstring": top_bitstring,
            "qubits": qubits,
            "histogram": Value::Object(histogram),
        }))
    }

    pub async fn run_quantum_ibm(
        cfg: &VeerOsConfig,
        req: &QuantumIbmRunRequest,
    ) -> Result<CommandOutput> {
        let mut args: Vec<String> = vec![
            "quantum".into(),
            "ibm-run".into(),
            "--backend".into(),
            req.backend.clone(),
            "--shots".into(),
            req.shots.to_string(),
            "--poll-ms".into(),
            req.poll_ms.to_string(),
            "--timeout-sec".into(),
            req.timeout_sec.to_string(),
        ];

        if let Some(program) = req.program {
            args.push("--program".into());
            args.push(program.as_cli_value().into());
        }
        if let Some(qasm_file) = req.qasm_file.as_ref() {
            args.push("--qasm-file".into());
            args.push(qasm_file.clone());
        }
        if let Some(token) = req.token.as_ref() {
            args.push("--token".into());
            args.push(token.clone());
        }
        if let Some(instance) = req.instance.as_ref() {
            args.push("--instance".into());
            args.push(instance.clone());
        }
        if req.histogram {
            args.push("--histogram".into());
        }
        if req.json {
            args.push("--json".into());
        }
        if let Some(endpoint) = req.endpoint {
            args.push("--endpoint".into());
            args.push(endpoint.as_cli_value().into());
        }

        Self::run_veer_command(cfg, &args).await
    }

    async fn run_vm_command(cfg: &VeerOsConfig, args: &[&str]) -> Result<CommandOutput> {
        if !Path::new(&cfg.vm_script_path).exists() {
            return Err(EdgeFabricError::Internal(format!(
                "VeerOS VM script not found: {}",
                cfg.vm_script_path
            )));
        }
        let output = Command::new(&cfg.vm_script_path)
            .current_dir(&cfg.root_dir)
            .args(args)
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_veer_command(cfg: &VeerOsConfig, args: &[String]) -> Result<CommandOutput> {
        if !Path::new(&cfg.veer_cli_path).exists() {
            return Err(EdgeFabricError::Internal(format!(
                "VeerOS CLI not found: {}",
                cfg.veer_cli_path
            )));
        }

        let output = Command::new(&cfg.veer_cli_path)
            .current_dir(&cfg.root_dir)
            .args(args)
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    fn build_circuit(program: Option<QuantumProgram>, qasm: Option<&str>) -> Result<Circuit> {
        if let Some(qasm_text) = qasm {
            Self::parse_quantum_circuit_from_qasm(qasm_text)
        } else {
            Ok(Self::build_quantum_program(program.unwrap_or(QuantumProgram::Bell)))
        }
    }

    fn build_quantum_program(program: QuantumProgram) -> Circuit {
        match program {
            QuantumProgram::Bell => Circuit::new(2).h(0).cx(0, 1).measure_all(),
            QuantumProgram::Ghz3 => Circuit::new(3).h(0).cx(0, 1).cx(1, 2).measure_all(),
            QuantumProgram::Qft3 => Circuit::new(3)
                .h(0)
                .cx(0, 1)
                .h(1)
                .cx(1, 2)
                .h(2)
                .measure_all(),
        }
    }

    fn parse_quantum_circuit_from_qasm(qasm: &str) -> Result<Circuit> {
        let mut qubits = 2usize;

        for line in qasm.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("qubit[") && trimmed.contains(']') {
                if let (Some(start), Some(end)) = (trimmed.find('['), trimmed.find(']')) {
                    if let Ok(count) = trimmed[start + 1..end].parse::<usize>() {
                        qubits = count;
                    }
                }
            }
        }

        let mut circuit = Circuit::new(qubits as u16);
        for line in qasm.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("OPENQASM")
                || trimmed.starts_with("include")
                || trimmed.starts_with("qubit[")
                || trimmed.starts_with("bit[")
            {
                continue;
            }

            if let Some(gate_part) = trimmed.split(' ').next() {
                match gate_part {
                    "h" | "x" | "y" | "z" => {
                        if let Some(idx) = Self::extract_qubit_index(trimmed) {
                            circuit = match gate_part {
                                "h" => circuit.h(idx as u16),
                                "x" => circuit.x(idx as u16),
                                "y" => circuit.y(idx as u16),
                                "z" => circuit.z(idx as u16),
                                _ => circuit,
                            };
                        }
                    }
                    "cx" => {
                        if let Some((q0, q1)) = Self::extract_two_qubit_indices(trimmed) {
                            circuit = circuit.cx(q0 as u16, q1 as u16);
                        }
                    }
                    "measure" => {
                        if let Some(idx) = Self::extract_qubit_index(trimmed) {
                            circuit = circuit.measure(idx as u16);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(circuit)
    }

    fn extract_qubit_index(line: &str) -> Option<usize> {
        if let Some(start) = line.find("q[") {
            if let Some(end) = line[start..].find(']') {
                return line[start + 2..start + end].parse::<usize>().ok();
            }
        }
        None
    }

    fn extract_two_qubit_indices(line: &str) -> Option<(usize, usize)> {
        let mut indices = Vec::new();
        let mut current = 0usize;
        while current < line.len() {
            if let Some(start) = line[current..].find("q[") {
                if let Some(end) = line[current + start..].find(']') {
                    if let Ok(idx) = line[current + start + 2..current + start + end].parse::<usize>() {
                        indices.push(idx);
                        current += start + end + 1;
                        if indices.len() == 2 {
                            return Some((indices[0], indices[1]));
                        }
                    } else {
                        current += start + 1;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        None
    }

    fn histogram_key_to_bitstring(key: u16, width: usize) -> String {
        let mut bits = String::with_capacity(width.max(1));
        let mut index = width;
        while index > 0 {
            index -= 1;
            let bit = if ((key >> index) & 1) == 1 { '1' } else { '0' };
            bits.push(bit);
        }
        bits
    }
}