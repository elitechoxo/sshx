# sshx — Simple TCP Tunnel

Expose any local port through your VPS with a named subdomain.

```
sshx -s myapp -p 3000          # HTTP
sshx -s myssh -p 22 --tcp      # SSH / raw TCP
```

---

## How It Works

```
Your Machine          Your VPS (sshx-server)        Internet
sshx client  ──────►  :7835  (control)   ◄──────  anyone
             ◄──────►  :XXXX (tunnel)    ◄──────  ssh myssh.yourdomain.com -p XXXX
```

1. Client connects to server on port 7835 and registers a subdomain.
2. Server binds a random public port and tells the client.
3. When anyone connects to that public port, server notifies client.
4. Client opens a second connection, server splices them together.
5. Raw TCP bytes flow bidirectionally — zero overhead.

---

## Server Setup (VPS)

### Requirements
- VPS with Docker + Docker Compose
- Wildcard DNS: `*.teamxpirates.qzz.io → your VPS IP`
- Open ports: `7835` (control) + your tunnel range (e.g. `2000-9000`)

### Deploy

```bash
git clone https://github.com/elitechoxo/SSHX-tunnel
cd SSHX-tunnel

# Optional: set a secret so only your clients can connect
# Edit docker-compose.yml → SSHX_SECRET=yourpassword

docker compose up -d
```

### Firewall (ufw example)
```bash
ufw allow 7835/tcp
ufw allow 2000:9000/tcp
```

---

## Client Install

### From source
```bash
cargo install --path client
```

### Pre-built binary (once you publish releases)
```bash
curl -fsSL https://yourserver/install.sh | sh
```

---

## Client Usage

```bash
# Expose a web app on port 3000
sshx -s myapp -p 3000

# Expose SSH on port 22 (raw TCP)
sshx -s myssh -p 22 --tcp

# With a secret
sshx -s myapp -p 3000 --secret yourpassword

# Custom server
sshx -s myapp -p 3000 --server your.server.com

# Disable auto-reconnect
sshx -s myapp -p 3000 --reconnect false
```

Output:
```
  ✓  Tunnel active!
     Subdomain : myapp.teamxpirates.qzz.io
     Public    : teamxpirates.qzz.io:4521
     Local     : localhost:3000
     Protocol  : Http
```

---

## Environment Variables

| Variable | Description |
|---|---|
| `SSHX_SERVER` | Server address (client) |
| `SSHX_SECRET` | Shared secret (client + server) |
| `SSHX_MIN_PORT` | Min tunnel port (server) |
| `SSHX_MAX_PORT` | Max tunnel port (server) |
| `SSHX_BIND` | Bind address (server) |
| `RUST_LOG` | Log level: `info`, `debug`, `warn` |

---

## DNS Setup

Add one wildcard A record in your DNS provider:

```
Type  : A
Name  : *
Value : <your VPS IP>
TTL   : 3600
```

This makes `anything.teamxpirates.qzz.io` point to your server.
The subdomain is just a label — actual routing is by port number.

---

## Security Notes

- Without `--secret`, anyone who knows your server address can open a tunnel.
- Set `SSHX_SECRET` in docker-compose.yml and pass `--secret` on client.
- Auth uses HMAC-SHA256 challenge-response — secret never sent in plain text.
- Tunnel ports are randomly assigned from your configured range.

---

## Project Structure

```
sshx/
├── server/          # sshx-server binary (runs on VPS)
│   └── src/
│       ├── main.rs      # server logic
│       ├── auth.rs      # HMAC auth
│       └── shared.rs    # protocol types + framing
├── client/          # sshx binary (runs on user machine)
│   └── src/
│       ├── main.rs      # client logic + CLI
│       ├── auth.rs      # HMAC auth (client side)
│       └── shared.rs    # protocol types + framing
├── Dockerfile           # server Docker image
├── docker-compose.yml   # easy server deployment
└── Cargo.toml           # workspace
```
