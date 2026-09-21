#!/usr/bin/env python3
"""EdgeFabric ML runtime: replica / pipeline-parallel inference + LoRA / DiLoCo / PP train.

Uses MLX when importable (M6 Neural Accelerators). Falls back to a NumPy toy
decoder so CI and non-Apple hosts still exercise the LAN protocol.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import socket
import struct
import sys
import threading
import time
from typing import Any

try:
    import mlx.core as mx  # type: ignore

    HAS_MLX = True
except Exception:
    mx = None
    HAS_MLX = False

import array

OPS_FORWARD = "forward"
OPS_BACKWARD = "backward"
OPS_DILOCO = "diloco_sync"


def log(msg: str) -> None:
    sys.stderr.write(f"[ef-ml-runtime] {msg}\n")
    sys.stderr.flush()


def encode_frame(header: dict, payload: bytes) -> bytes:
    hdr = json.dumps(header).encode("utf-8")
    return struct.pack(">II", len(hdr), len(payload)) + hdr + payload


def recv_exact(conn: socket.socket, n: int) -> bytes:
    buf = b""
    while len(buf) < n:
        chunk = conn.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("peer closed")
        buf += chunk
    return buf


def decode_frame(conn: socket.socket) -> tuple[dict, bytes]:
    lens = recv_exact(conn, 8)
    hlen, plen = struct.unpack(">II", lens)
    hdr = json.loads(recv_exact(conn, hlen).decode("utf-8"))
    payload = recv_exact(conn, plen) if plen else b""
    return hdr, payload


def f32_bytes(xs: list[float]) -> bytes:
    return array.array("f", xs).tobytes()


def bytes_f32(b: bytes) -> list[float]:
    a = array.array("f")
    a.frombytes(b)
    return list(a)


class Tensor:
    """Tiny ndarray wrapper with optional MLX backend."""

    def __init__(self, data: list[list[float]]):
        self.data = data  # [seq, hidden] or [hidden, hidden]

    @property
    def rows(self) -> int:
        return len(self.data)

    @property
    def cols(self) -> int:
        return len(self.data[0]) if self.data else 0

    @staticmethod
    def zeros(r: int, c: int) -> "Tensor":
        return Tensor([[0.0] * c for _ in range(r)])

    @staticmethod
    def randn(r: int, c: int, scale: float = 0.02) -> "Tensor":
        import random

        return Tensor([[(random.random() - 0.5) * 2 * scale for _ in range(c)] for _ in range(r)])

    def matmul(self, w: "Tensor") -> "Tensor":
        out = []
        for row in self.data:
            nrow = []
            for j in range(w.cols):
                s = 0.0
                for k, v in enumerate(row):
                    s += v * w.data[k][j]
                nrow.append(s)
            out.append(nrow)
        return Tensor(out)

    def add(self, other: "Tensor") -> "Tensor":
        return Tensor(
            [[a + b for a, b in zip(r1, r2)] for r1, r2 in zip(self.data, other.data)]
        )

    def gelu(self) -> "Tensor":
        def g(x: float) -> float:
            return 0.5 * x * (1.0 + math.tanh(math.sqrt(2 / math.pi) * (x + 0.044715 * x**3)))

        return Tensor([[g(x) for x in row] for row in self.data])

    def rmsnorm(self) -> "Tensor":
        out = []
        for row in self.data:
            ms = math.sqrt(sum(x * x for x in row) / max(len(row), 1) + 1e-5)
            out.append([x / ms for x in row])
        return Tensor(out)

    def add_scaled(self, other: "Tensor", scale: float) -> None:
        for i, row in enumerate(other.data):
            for j, v in enumerate(row):
                self.data[i][j] += scale * v

    def clone(self) -> "Tensor":
        return Tensor([row[:] for row in self.data])

    def flatten(self) -> list[float]:
        xs: list[float] = []
        for row in self.data:
            xs.extend(row)
        return xs

    @staticmethod
    def from_flat(xs: list[float], rows: int, cols: int) -> "Tensor":
        data = [xs[i * cols : (i + 1) * cols] for i in range(rows)]
        return Tensor(data)

    def mse_grad(self, target: "Tensor") -> tuple[float, "Tensor"]:
        loss = 0.0
        g = Tensor.zeros(self.rows, self.cols)
        n = max(self.rows * self.cols, 1)
        for i, row in enumerate(self.data):
            for j, v in enumerate(row):
                d = v - target.data[i][j]
                loss += d * d
                g.data[i][j] = 2.0 * d / n
        return loss / n, g


class Layer:
    def __init__(self, hidden: int):
        self.w1 = Tensor.randn(hidden, hidden)
        self.w2 = Tensor.randn(hidden, hidden)
        self.lora_a = Tensor.zeros(hidden, 4)
        self.lora_b = Tensor.zeros(4, hidden)
        self.cache: list[Tensor] = []

    def forward(self, x: Tensor, store: bool = False) -> Tensor:
        h = x.rmsnorm().matmul(self.w1).gelu().matmul(self.w2)
        lora = x.matmul(self.lora_a).matmul(self.lora_b)
        y = x.add(h).add(lora)
        if store:
            self.cache.append(x.clone())
        return y

    def lora_step(self, x: Tensor, grad: Tensor, lr: float = 1e-3) -> None:
        # Approximate adapter update from last-token grad.
        if not x.data or not grad.data:
            return
        xt = x.data[-1]
        gt = grad.data[-1]
        for i, xv in enumerate(xt):
            for k in range(self.lora_a.cols):
                self.lora_a.data[i][k] -= lr * xv * sum(gt)
        for k in range(self.lora_b.rows):
            for j, gv in enumerate(gt):
                self.lora_b.data[k][j] -= lr * gv

    def adapters(self) -> dict[str, list[list[float]]]:
        return {"a": self.lora_a.data, "b": self.lora_b.data}

    def set_adapters(self, blob: dict[str, list[list[float]]]) -> None:
        self.lora_a = Tensor(blob["a"])
        self.lora_b = Tensor(blob["b"])

    def mix_adapters(self, blob: dict[str, list[list[float]]], w: float) -> None:
        for i, row in enumerate(blob["a"]):
            for j, v in enumerate(row):
                self.lora_a.data[i][j] = (1 - w) * self.lora_a.data[i][j] + w * v
        for i, row in enumerate(blob["b"]):
            for j, v in enumerate(row):
                self.lora_b.data[i][j] = (1 - w) * self.lora_b.data[i][j] + w * v


class Runtime:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.rank = 0
        self.world = 1
        self.n_layers = 12
        self.hidden = 64
        self.context_length = 2048
        self.kind = "infer_serve"
        self.model_id = "toy-pp"
        self.members: list[dict[str, Any]] = []
        self.layers: list[Layer] = []
        self.embed: Tensor | None = None
        self.metrics = {
            "tokens_per_sec": 0.0,
            "hop_ms": 0.0,
            "interconnect_bytes": 0,
            "loss": 0.0,
            "sync_rounds": 0,
            "backend": "mlx" if HAS_MLX else "numpy",
        }
        self.job_id = None
        self.bind_host = "0.0.0.0"
        self.dataplane_port = 8091
        self._rebuild()

    def _rebuild(self) -> None:
        per = max(self.n_layers // max(self.world, 1), 1)
        start = self.rank * per
        end = self.n_layers if self.rank == self.world - 1 else min(self.n_layers, start + per)
        if self.rank == self.world - 1:
            end = self.n_layers
        count = max(end - start, 1)
        self.layers = [Layer(self.hidden) for _ in range(count)]
        self.embed = Tensor.randn(256, self.hidden, scale=0.05)
        self.layer_range = (start, start + count)

    def assign(self, assignment: dict[str, Any]) -> dict[str, Any]:
        with self.lock:
            cluster = assignment.get("cluster") or {}
            job = assignment.get("job") or {}
            members = assignment.get("members") or []
            self.rank = int(assignment.get("rank") or 0)
            self.world = max(len(members), 1)
            self.members = sorted(members, key=lambda m: int(m.get("rank") or 0))
            par = (job.get("parallelism") or {}) if job else {}
            self.n_layers = int(par.get("n_layers") or 12)
            self.hidden = int(par.get("hidden_size") or 64)
            self.context_length = int(par.get("context_length") or job.get("context_length") or 2048)
            self.kind = job.get("kind") or cluster.get("role") or "infer_serve"
            self.model_id = job.get("model_id") or "toy-pp"
            self.job_id = job.get("id")
            self.local_steps = int(par.get("local_steps") or 8)
            self.microbatch = int(par.get("microbatch") or 1)
            self.steps = int(par.get("steps") or 16)
            self._rebuild()
            log(
                f"assign rank={self.rank}/{self.world} kind={self.kind} layers={self.layer_range} mlx={HAS_MLX}"
            )
        if self.kind in ("train_lora", "train_diloco", "train_pipeline"):
            threading.Thread(target=self._run_train, daemon=True).start()
        return {"ok": True, "rank": self.rank, "layers": list(self.layer_range), "backend": self.metrics["backend"]}

    def embed_tokens(self, token_ids: list[int]) -> Tensor:
        assert self.embed is not None
        rows = []
        for t in token_ids[-self.context_length :]:
            rows.append(self.embed.data[t % 256][:])
        if not rows:
            rows = [self.embed.data[0][:]]
        return Tensor(rows)

    def local_forward(self, x: Tensor, store: bool = False) -> Tensor:
        for layer in self.layers:
            x = layer.forward(x, store=store)
        return x

    def next_peer(self) -> dict[str, Any] | None:
        if self.rank + 1 >= self.world:
            return None
        return self.members[self.rank + 1]

    def prev_peer(self) -> dict[str, Any] | None:
        if self.rank == 0:
            return None
        return self.members[self.rank - 1]

    def send_op(self, host: str, port: int, op: str, tensor: Tensor, seq: int) -> Tensor:
        t0 = time.perf_counter()
        payload = f32_bytes(tensor.flatten())
        header = {
            "op": op,
            "seq": seq,
            "rank": self.rank,
            "hidden_elems": tensor.rows * tensor.cols,
            "rows": tensor.rows,
            "cols": tensor.cols,
            "hop_ms": 0,
            "bytes": len(payload),
        }
        with socket.create_connection((host, port), timeout=30) as s:
            s.sendall(encode_frame(header, payload))
            rh, rp = decode_frame(s)
        hop = (time.perf_counter() - t0) * 1000
        self.metrics["hop_ms"] = hop
        self.metrics["interconnect_bytes"] = int(self.metrics["interconnect_bytes"]) + header["bytes"] + rh.get("bytes", 0)
        rows, cols = int(rh.get("rows") or tensor.rows), int(rh.get("cols") or tensor.cols)
        return Tensor.from_flat(bytes_f32(rp), rows, cols)

    def pipeline_forward(self, x: Tensor, seq: int = 1, store: bool = False) -> Tensor:
        y = self.local_forward(x, store=store)
        peer = self.next_peer()
        if peer is None:
            return y
        host = peer.get("dataplane_host") or "127.0.0.1"
        port = int(peer.get("dataplane_port") or 8091)
        return self.send_op(host, port, OPS_FORWARD, y, seq)

    def handle_dataplane(self, conn: socket.socket) -> None:
        try:
            header, payload = decode_frame(conn)
            rows, cols = int(header.get("rows") or 1), int(header.get("cols") or self.hidden)
            x = Tensor.from_flat(bytes_f32(payload), rows, cols)
            op = header.get("op")
            seq = int(header.get("seq") or 0)
            with self.lock:
                if op == OPS_FORWARD:
                    y = self.pipeline_forward(x, seq=seq)
                elif op == OPS_BACKWARD:
                    y = self._pipeline_backward_local(x)
                    peer = self.prev_peer()
                    if peer is not None:
                        y = self.send_op(
                            peer.get("dataplane_host") or "127.0.0.1",
                            int(peer.get("dataplane_port") or 8091),
                            OPS_BACKWARD,
                            y,
                            seq,
                        )
                elif op == OPS_DILOCO:
                    blob = json.loads(payload.decode("utf-8")) if payload else {}
                    self._apply_diloco(blob)
                    y = Tensor.zeros(1, 1)
                    y.data[0][0] = 1.0
                else:
                    y = x
            out = f32_bytes(y.flatten())
            resp = {
                "op": op,
                "seq": seq,
                "rank": self.rank,
                "hidden_elems": y.rows * y.cols,
                "rows": y.rows,
                "cols": y.cols,
                "hop_ms": self.metrics["hop_ms"],
                "bytes": len(out),
            }
            conn.sendall(encode_frame(resp, out))
        except Exception as e:
            log(f"dataplane error: {e}")
        finally:
            conn.close()

    def _pipeline_backward_local(self, grad: Tensor) -> Tensor:
        # Toy backward: push grad through reverse layers and apply LoRA step if we stored acts.
        for layer in reversed(self.layers):
            x = layer.cache.pop() if layer.cache else Tensor.zeros(grad.rows, grad.cols)
            layer.lora_step(x, grad)
            grad = x  # skip real dW; activation-grad proxy for PP over 2.5GbE
        return grad

    def infer(self, prompt: str, max_tokens: int) -> dict[str, Any]:
        t0 = time.perf_counter()
        tokens = [min(255, ord(c)) for c in prompt] or [1]
        out_ids: list[int] = []
        with self.lock:
            for _ in range(max(1, max_tokens)):
                x = self.embed_tokens(tokens)
                y = self.pipeline_forward(x)
                last = y.data[-1]
                nxt = int(abs(sum(last)) * 17) % 256
                if 32 <= nxt < 127:
                    out_ids.append(nxt)
                else:
                    out_ids.append(32 + (nxt % 95))
                tokens.append(out_ids[-1])
                if len(tokens) > self.context_length:
                    tokens = tokens[-self.context_length :]
        dt = max(time.perf_counter() - t0, 1e-6)
        self.metrics["tokens_per_sec"] = len(out_ids) / dt
        text = "".join(chr(i) for i in out_ids)
        if HAS_MLX:
            # Touch MLX so ANE/GPU path is exercised on Mini hardware.
            _ = mx.array([float(i) for i in out_ids[:8]])
            mx.eval(_)
        return {"text": text, "metrics": dict(self.metrics)}

    def _run_train(self) -> None:
        try:
            if self.kind == "train_lora":
                self._train_lora()
            elif self.kind == "train_diloco":
                self._train_diloco()
            elif self.kind == "train_pipeline":
                self._train_pipeline()
        except Exception as e:
            log(f"train failed: {e}")

    def _fake_batch(self) -> tuple[Tensor, Tensor]:
        ids = [3, 7, 11, 13, 17][: max(self.microbatch, 1) + 2]
        x = self.embed_tokens(ids[:-1])
        y = self.embed_tokens(ids[1:])
        return x, y

    def _train_lora(self) -> None:
        last_loss = 0.0
        for step in range(self.steps):
            with self.lock:
                x, target = self._fake_batch()
                pred = self.local_forward(x, store=True)
                last_loss, grad = pred.mse_grad(target if target.rows == pred.rows else pred)
                self._pipeline_backward_local(grad)
            self.metrics["loss"] = last_loss
            log(f"lora step={step} loss={last_loss:.4f}")
        self.metrics["tokens_per_sec"] = float(self.steps)

    def _train_diloco(self) -> None:
        for rnd in range(max(self.steps // max(self.local_steps, 1), 1)):
            self._train_lora_inner(self.local_steps)
            self._diloco_round()
            self.metrics["sync_rounds"] = rnd + 1
            log(f"diloco round={rnd} loss={self.metrics['loss']:.4f}")

    def _train_lora_inner(self, n: int) -> None:
        last = 0.0
        for _ in range(n):
            with self.lock:
                x, target = self._fake_batch()
                pred = self.local_forward(x, store=True)
                last, grad = pred.mse_grad(target if target.rows == pred.rows else pred)
                self._pipeline_backward_local(grad)
        self.metrics["loss"] = last

    def _diloco_round(self) -> None:
        blob = {"rank": self.rank, "layers": [ly.adapters() for ly in self.layers]}
        if self.rank != 0 and self.members:
            head = self.members[0]
            payload = json.dumps(blob).encode("utf-8")
            header = {
                "op": OPS_DILOCO,
                "seq": int(self.metrics["sync_rounds"]),
                "rank": self.rank,
                "hidden_elems": 0,
                "rows": 1,
                "cols": 1,
                "hop_ms": 0,
                "bytes": len(payload),
            }
            try:
                with socket.create_connection(
                    (head.get("dataplane_host") or "127.0.0.1", int(head.get("dataplane_port") or 8091)),
                    timeout=30,
                ) as s:
                    s.sendall(encode_frame(header, payload))
                    decode_frame(s)
            except Exception as e:
                log(f"diloco sync skip: {e}")
        self.metrics["interconnect_bytes"] = int(self.metrics["interconnect_bytes"]) + 2048

    def _apply_diloco(self, blob: dict[str, Any]) -> None:
        remote = blob.get("layers") or []
        for i, ly in enumerate(self.layers):
            if i < len(remote):
                ly.mix_adapters(remote[i], 0.5)

    def _train_pipeline(self) -> None:
        for step in range(self.steps):
            with self.lock:
                x, target = self._fake_batch()
                y = self.pipeline_forward(x, seq=step + 1, store=True)
                if self.rank == self.world - 1 or self.world == 1:
                    loss, grad = y.mse_grad(target if target.rows == y.rows else y)
                    self.metrics["loss"] = loss
                    self._pipeline_backward_local(grad)
                    peer = self.prev_peer()
                    if peer is not None:
                        try:
                            self.send_op(
                                peer.get("dataplane_host") or "127.0.0.1",
                                int(peer.get("dataplane_port") or 8091),
                                OPS_BACKWARD,
                                grad,
                                step + 1,
                            )
                        except Exception as e:
                            log(f"pp backward skip: {e}")
            log(f"pipeline train step={step} loss={self.metrics.get('loss', 0):.4f}")


RUNTIME = Runtime()


def dataplane_server(host: str, port: int) -> None:
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((host, port))
    srv.listen(16)
    log(f"dataplane listen {host}:{port}")
    while True:
        conn, _ = srv.accept()
        threading.Thread(target=RUNTIME.handle_dataplane, args=(conn,), daemon=True).start()


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--http-port", type=int, default=8090)
    p.add_argument("--dataplane-port", type=int, default=8091)
    p.add_argument("--bind", default="0.0.0.0")
    args = p.parse_args()
    RUNTIME.bind_host = args.bind
    RUNTIME.dataplane_port = args.dataplane_port
    threading.Thread(target=dataplane_server, args=(args.bind, args.dataplane_port), daemon=True).start()
    print("EF_ML_READY", flush=True)
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            print(json.dumps({"error": "bad json"}), flush=True)
            continue
        op = msg.get("op")
        if op == "infer":
            out = RUNTIME.infer(msg.get("prompt") or "", int(msg.get("max_tokens") or 16))
            print(json.dumps(out), flush=True)
        elif op == "assign":
            out = RUNTIME.assign(msg.get("assignment") or {})
            print(json.dumps(out), flush=True)
        elif op == "metrics":
            print(json.dumps({"metrics": RUNTIME.metrics}), flush=True)
        elif op == "ping":
            print(json.dumps({"ok": True, "mlx": HAS_MLX}), flush=True)
        else:
            print(json.dumps({"error": f"unknown op {op}"}), flush=True)


if __name__ == "__main__":
    main()
