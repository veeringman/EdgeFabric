# ef-ml-worker

Supervisor for M6 Mini (and compatible) hosts. Spawns `runtime/mlx_runtime.py` (MLX if present, NumPy toy model otherwise).

- Heartbeats to EdgeFabric `/api/v1/ml/nodes/heartbeat` and loads cluster assignments
- OpenAI-compatible HTTP on `--http-port` (rank-0)
- LAN dataplane on `--dataplane-port` (`ForwardActivation` / `BackwardGrad` / `DiLoCoSync`)

```bash
ef-ml-worker \
  --control-url http://127.0.0.1:8080 \
  --token "$EF_TOKEN" \
  --runner-id "$EF_RUNNER_ID" \
  --worker-host 192.168.29.41
```

Replica `infer_serve` runs the full toy stack on one node. Pipeline mode splits layers across ranks. `train_lora`, `train_diloco`, and `train_pipeline` run after assignment.
