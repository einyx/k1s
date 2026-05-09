# k1s Vault with etcd Backend (k3s Compatible)

## Overview

Use etcd as the vault storage backend for distributed, strongly consistent secret management - exactly like k3s does for Kubernetes resources.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   k1s Cluster                        │
├─────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐          │
│  │  Node 1  │  │  Node 2  │  │  Node 3  │          │
│  │          │  │          │  │          │          │
│  │  Vault   │  │  Vault   │  │  Vault   │          │
│  │  ▲       │  │  ▲       │  │  ▲       │          │
│  └──┼───────┘  └──┼───────┘  └──┼───────┘          │
│     │            │            │                     │
│     └────────────┼────────────┘                     │
│                  │                                   │
│     ┌────────────▼────────────┐                     │
│     │    etcd Cluster         │                     │
│     │  (Raft Consensus)       │                     │
│     │                         │                     │
│     │  • Strong Consistency   │                     │
│     │  • Automatic Failover   │                     │
│     │  • Watch/Lease Support  │                     │
│     └─────────────────────────┘                     │
└─────────────────────────────────────────────────────┘
```

## Benefits

### Why etcd for Vault?

1. **k3s Compatibility** - Same data store as k3s uses for k8s resources
2. **Distributed** - Multi-node clusters with automatic failover
3. **Strong Consistency** - Raft consensus, no split-brain
4. **Battle-Tested** - Production-proven by Kubernetes ecosystem
5. **Watch Support** - Real-time secret updates
6. **Lease Support** - Automatic secret expiration/cleanup

### Comparison with Sled

| Feature | Sled (Embedded) | etcd (Distributed) |
|---------|----------------|-------------------|
| Setup | Single binary | Requires etcd cluster |
| Consistency | Strong (single-node) | Strong (multi-node) |
| Availability | Single point of failure | High availability |
| Scalability | Limited to one node | Horizontal scaling |
| Watch | Not supported | Native support |
| Use Case | Development, edge | Production, multi-node |

## Configuration

### 1. Using k3s Built-in etcd

If you're running k3s with embedded etcd:

```bash
# k3s automatically runs etcd on port 2379
# Vault can share the same etcd cluster

k1s server \
  --vault-backend etcd \
  --etcd-endpoints http://127.0.0.1:2379 \
  --etcd-prefix /k1s/vault \
  --data-dir /var/lib/k1s
```

### 2. External etcd Cluster

For dedicated etcd cluster:

```bash
# Start etcd cluster (3 nodes for HA)
etcd --name node1 \
  --data-dir /var/lib/etcd \
  --listen-peer-urls http://0.0.0.0:2380 \
  --listen-client-urls http://0.0.0.0:2379 \
  --advertise-client-urls http://node1:2379 \
  --initial-cluster node1=http://node1:2380,node2=http://node2:2380,node3=http://node3:2380

# Configure k1s to use it
k1s server \
  --vault-backend etcd \
  --etcd-endpoints http://node1:2379,http://node2:2379,http://node3:2379 \
  --etcd-prefix /k1s/vault
```

### 3. TLS with etcd

For production with TLS:

```bash
k1s server \
  --vault-backend etcd \
  --etcd-endpoints https://etcd1:2379,https://etcd2:2379,https://etcd3:2379 \
  --etcd-ca-cert /etc/etcd/ca.crt \
  --etcd-cert /etc/etcd/client.crt \
  --etcd-key /etc/etcd/client.key \
  --etcd-prefix /k1s/vault
```

## Implementation

### Vault Initialization

```rust
use k1s_vault::Vault;
use k1s_vault::storage::etcd_backend::EtcdStorage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to etcd (k3s or standalone)
    let storage = EtcdStorage::new(
        vec!["http://127.0.0.1:2379".to_string()],
        Some("/k1s/vault".to_string())
    ).await?;

    // Initialize vault with etcd backend
    let vault = Vault::with_storage(Arc::new(storage))?;

    // All operations now use etcd
    vault.kv.write("database/prod", data, None, auth).await?;

    Ok(())
}
```

### Auto-detection of k3s etcd

```rust
impl EtcdStorage {
    /// Automatically detect and connect to k3s etcd
    pub async fn from_k3s() -> VaultResult<Self> {
        // Check if k3s is running
        if std::path::Path::new("/var/lib/rancher/k3s").exists() {
            // Use k3s embedded etcd
            return Self::new(
                vec!["http://127.0.0.1:2379".to_string()],
                Some("/k1s/vault".to_string())
            ).await;
        }

        Err(VaultError::Internal("k3s not detected".to_string()))
    }
}
```

## Data Layout in etcd

Vault stores data with the following key structure:

```
/k1s/vault/
├── kv/
│   ├── metadata/
│   │   └── database/
│   │       └── prod                    # SecretMetadata
│   └── data/
│       └── database/
│           └── prod/
│               ├── v1                  # SecretVersion
│               └── v2                  # SecretVersion
├── transit/
│   └── keys/
│       └── app-key                     # TransitKey
├── pki/
│   ├── ca/
│   │   └── k1s-ca                      # CertificateAuthority
│   └── certs/
│       └── k1s-ca/
│           └── 0000000000000001        # IssuedCertificate
└── audit/
    └── log/
        └── 1645456789000/
            └── uuid                    # AuditEntry
```

## Watch Support for Real-time Updates

With etcd, vault can watch for secret changes:

```rust
use futures::StreamExt;

// Watch for KV changes
let mut watch_stream = storage.watch("kv/data/").await?;

while let Some(event) = watch_stream.next().await {
    match event {
        WatchEvent::Put { key, value } => {
            println!("Secret updated: {}", key);
            // Trigger pod restart, webhook notification, etc.
        }
        WatchEvent::Delete { key } => {
            println!("Secret deleted: {}", key);
        }
    }
}
```

### Use Cases for Watch

1. **Automatic Pod Restarts** - Restart pods when secrets change
2. **Secret Injection** - Update mounted secrets without pod restart
3. **Audit Triggers** - Real-time security monitoring
4. **Replication** - Sync to external systems

## Migration from Sled to etcd

### Zero-Downtime Migration

```bash
#!/bin/bash
# Migrate vault data from Sled to etcd

# 1. Export from Sled
k1s vault export --backend sled --output /tmp/vault-backup.json

# 2. Stop k1s server
systemctl stop k1s

# 3. Start with etcd backend
k1s server --vault-backend etcd --etcd-endpoints http://localhost:2379 &

# 4. Import to etcd
k1s vault import --backend etcd --input /tmp/vault-backup.json

# 5. Verify
k1s vault audit logs --backend etcd
```

### Backup Script

```bash
#!/bin/bash
# Backup vault data from etcd

ETCDCTL_API=3 etcdctl \
  --endpoints=http://127.0.0.1:2379 \
  get /k1s/vault/ --prefix \
  --print-value-only > /backup/vault-$(date +%Y%m%d).backup
```

## High Availability Setup

### 3-Node etcd Cluster

```yaml
# docker-compose.yml for HA etcd
version: '3'
services:
  etcd1:
    image: quay.io/coreos/etcd:v3.5.9
    environment:
      - ETCD_NAME=etcd1
      - ETCD_INITIAL_CLUSTER=etcd1=http://etcd1:2380,etcd2=http://etcd2:2380,etcd3=http://etcd3:2380
      - ETCD_INITIAL_CLUSTER_STATE=new
      - ETCD_INITIAL_ADVERTISE_PEER_URLS=http://etcd1:2380
      - ETCD_ADVERTISE_CLIENT_URLS=http://etcd1:2379
      - ETCD_LISTEN_CLIENT_URLS=http://0.0.0.0:2379
      - ETCD_LISTEN_PEER_URLS=http://0.0.0.0:2380
    ports:
      - 2379:2379
      - 2380:2380

  etcd2:
    image: quay.io/coreos/etcd:v3.5.9
    environment:
      - ETCD_NAME=etcd2
      - ETCD_INITIAL_CLUSTER=etcd1=http://etcd1:2380,etcd2=http://etcd2:2380,etcd3=http://etcd3:2380
      - ETCD_INITIAL_CLUSTER_STATE=new
      - ETCD_INITIAL_ADVERTISE_PEER_URLS=http://etcd2:2380
      - ETCD_ADVERTISE_CLIENT_URLS=http://etcd2:2379
      - ETCD_LISTEN_CLIENT_URLS=http://0.0.0.0:2379
      - ETCD_LISTEN_PEER_URLS=http://0.0.0.0:2380

  etcd3:
    image: quay.io/coreos/etcd:v3.5.9
    environment:
      - ETCD_NAME=etcd3
      - ETCD_INITIAL_CLUSTER=etcd1=http://etcd1:2380,etcd2=http://etcd2:2380,etcd3=http://etcd3:2380
      - ETCD_INITIAL_CLUSTER_STATE=new
      - ETCD_INITIAL_ADVERTISE_PEER_URLS=http://etcd3:2380
      - ETCD_ADVERTISE_CLIENT_URLS=http://etcd3:2379
      - ETCD_LISTEN_CLIENT_URLS=http://0.0.0.0:2379
      - ETCD_LISTEN_PEER_URLS=http://0.0.0.0:2380
```

### k1s Configuration

```bash
# Connect to HA etcd cluster
k1s server \
  --vault-backend etcd \
  --etcd-endpoints http://etcd1:2379,http://etcd2:2379,http://etcd3:2379 \
  --etcd-prefix /k1s/vault
```

## Performance Tuning

### etcd Optimization for Vault

```bash
# Increase etcd quota for large secrets
etcd --quota-backend-bytes 8589934592  # 8GB

# Optimize compaction
etcdctl compact $(etcdctl endpoint status --write-out="json" | jq -r '.[0].Status.header.revision')

# Defragment
etcdctl defrag
```

### Vault-Specific Settings

```rust
// Connection pooling
let client = Client::connect(endpoints, Some(ConnectOptions {
    timeout: Duration::from_secs(5),
    keep_alive: Some(Duration::from_secs(30)),
    ..Default::default()
})).await?;

// Batch operations for better performance
async fn batch_write(&self, items: Vec<(String, Vec<u8>)>) -> VaultResult<()> {
    let mut txn = self.client.txn();
    for (key, value) in items {
        txn = txn.and_then(etcd_client::TxnOpResponse::put(key, value));
    }
    txn.commit().await?;
    Ok(())
}
```

## Monitoring

### etcd Health Checks

```bash
# Check etcd cluster health
ETCDCTL_API=3 etcdctl endpoint health

# Check vault data size
ETCDCTL_API=3 etcdctl get /k1s/vault/ --prefix --count-only

# Monitor vault operations
ETCDCTL_API=3 etcdctl watch /k1s/vault/ --prefix
```

### Prometheus Metrics

```yaml
# Expose etcd metrics
scrape_configs:
  - job_name: 'etcd'
    static_configs:
      - targets: ['etcd1:2379', 'etcd2:2379', 'etcd3:2379']

  - job_name: 'k1s-vault'
    static_configs:
      - targets: ['k1s:6443']
    metrics_path: '/metrics'
```

## Security Best Practices

1. **TLS Everywhere** - Use TLS for etcd client and peer communication
2. **RBAC** - Enable etcd RBAC and create vault-specific user
3. **Network Isolation** - Firewall etcd ports, only accessible to k1s nodes
4. **Encryption at Rest** - Enable etcd encryption at rest
5. **Regular Backups** - Automated etcd snapshots
6. **Audit Logging** - Enable etcd audit logs

## Production Checklist

- [ ] 3+ node etcd cluster for HA
- [ ] TLS enabled for client and peer communication
- [ ] etcd RBAC configured with vault-specific user
- [ ] Automated backups configured (daily)
- [ ] Monitoring and alerting set up
- [ ] Firewall rules limiting etcd access
- [ ] Encryption at rest enabled
- [ ] Resource limits configured (memory, disk)
- [ ] Disaster recovery plan documented
- [ ] Performance baselines established

## Comparison: Sled vs etcd vs P2P Gossip

| Feature | Sled | etcd | P2P Gossip |
|---------|------|------|------------|
| Deployment | Single binary | etcd cluster | libp2p mesh |
| Consistency | Strong | Strong | Eventual |
| Availability | Single node | Multi-node | Multi-node |
| Latency | < 1ms | 5-20ms | 10-50ms |
| Setup | Zero | Medium | Low |
| k3s Compat | No | Yes ✅ | No |
| Watch | No | Yes ✅ | Yes |
| Use Case | Dev/Edge | Production | Mesh networks |

## Recommendation

**For k1s with k3s compatibility:**
- **Development**: Sled (embedded, zero setup)
- **Production**: etcd (k3s compatible, proven)
- **Edge/IoT**: P2P Gossip (mesh networking)

**Best Practice:** Start with Sled, migrate to etcd for production multi-node deployments, especially if already using k3s.
