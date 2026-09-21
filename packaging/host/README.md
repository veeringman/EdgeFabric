# EdgeFabric host worker — what to install on a Mac mini / MacBook / Linux / Windows box

Any internet host joins as a **generic SSH worker**. EdgeFabric (in the cloud) never needs your LAN IP.

## What the host must have

1. **OpenSSH server** (Remote Login on macOS, `openssh-server` on Linux, OpenSSH Server on Windows).
2. **These binaries** (this package):
   - `ef` — enroll: `ef host join --api-key …`
   - `veersetu-agent` — publishes local `:22` through VeerSetu so EC2 can SSH in
   - `ef-agent` — optional: phone/IoT discovery on this LAN
   - `ef-ml-worker` — optional: ML inference/training (`--with-ml`)
3. **Tokens** from the operator: EdgeFabric API key, VeerSetu control token.

## Install (macOS / Linux)

```bash
# from the unpacked directory
export EF_API_KEY=ef-...
export VEERSETU_CONTROL_TOKEN=...   # optional but required off-LAN
./install.sh
```

Or copy binaries to `~/.edgefabric/bin` and run:

```bash
export PATH="$HOME/.edgefabric/bin:$PATH"
ef --server https://edgefabric.veerlabs.solutions host join --api-key "$EF_API_KEY" --with-ml
```

Then start VeerSetu (service name is unique per host):

```bash
veersetu-agent \
  --relay 100.52.147.238:51820 \
  --server-name veersetu-rd.veerlabs.solutions \
  --control-url http://veersetu-rd.veerlabs.solutions:51821 \
  --service ssh-$(hostname) \
  --upstream 127.0.0.1:22 \
  --node-id $(hostname)
```

## Windows

Build `ef.exe` / `ef-agent.exe` / `ef-ml-worker.exe` with

`cargo build --release -p ef-cli -p ef-agent -p ef-ml-worker --target x86_64-pc-windows-msvc`

Install OpenSSH Server, then the same `ef host join` flow. Use `veersetu-agent.exe` from a Windows VeerSetu build.

## After join

The host appears under **Devices & Hosts**. ML Fabric lists it if ML caps are present. iOS/macOS/Linux/Windows jobs match on capabilities, not a special runner class.
