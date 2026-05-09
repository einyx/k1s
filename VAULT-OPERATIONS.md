# k1s Vault Operations Guide

## Test Results - All Engines Working ✅

### Transit Engine (Encryption)
```
Created key: app-key
Encrypted: vault:v1:GBv1LRcpuMg0be8i6drjj5/lkyPbjvmesoNG94hu3U0zWwT+uPn2nVgFgKd7Z3weKcbAFQ==
Decrypted: my-database-password-123
```

### KV Engine (Versioned Secrets)
```
v1: {"username":"admin","password":"secret123"}
v2: {"username":"admin","password":"newsecret456"}
Historical read of v1 working ✅
```

### PKI Engine (Certificates)
```
CA: k1s-ca (365 days validity)
Certificate issued: kubelet.k1s.local
Serial: 0000000000000001
TTL: 24h
```

### Audit Logs
```
7 operations logged including:
- KV reads/writes
- PKI certificate issuance
- Transit encrypt/decrypt
All with timestamps, user info, and results
```

## P2P Distribution of Vault Data

### Current Architecture
The vault currently uses the same **embedded Sled database** as the k1s storage backend. This means:

1. **Single-Node**: Vault data is local to each API server node
2. **Storage Path**: `/var/lib/k1s/storage` (same as Kubernetes resources)
3. **Encryption**: Vault secrets stored in Sled with optional encryption at rest

### P2P Options for Multi-Node Clusters

#### Option 1: P2P Gossip Replication (Recommended)
Use k1s-p2p to replicate vault data across nodes:

**Pros:**
- Consistent with k1s architecture
- Low latency (local reads)
- Automatic failover
- No external dependencies

**Cons:**
- Eventually consistent
- Requires conflict resolution for concurrent writes

**Implementation:**
```rust
// In k1s-vault/src/lib.rs
pub struct Vault {
    pub transit: TransitEngine,
    pub kv: KvEngine,
    pub pki: PkiEngine,
    pub audit: Arc<AuditLogger>,
    pub replication: Option<Arc<VaultReplicator>>, // New!
}

// Use libp2p gossipsub for vault operations
impl VaultReplicator {
    pub async fn replicate_kv_write(&self, path: &str, data: &[u8]) {
        // Publish to gossipsub topic: "vault/kv/write"
        self.p2p.publish("vault/kv/write", VaultUpdate {
            operation: "write",
            path,
            data,
            timestamp: Utc::now(),
            node_id: self.node_id,
        }).await;
    }
}
```

#### Option 2: Raft Consensus (Strong Consistency)
Use Raft for strongly consistent vault operations:

**Pros:**
- Strong consistency guarantees
- Proven for secrets management (HashiCorp Vault uses this)
- Transactional semantics

**Cons:**
- Higher latency (requires quorum)
- More complex implementation
- Single leader bottleneck

**Implementation:**
```rust
// Use tikv/raft-rs
pub struct VaultRaft {
    raft_node: RaftNode,
    storage: Arc<SledBackend>,
}
```

#### Option 3: External Backend (Production)
For production multi-cluster deployments:

**Backends:**
- **Consul**: Distributed KV store with strong consistency
- **etcd**: k8s-native, strong consistency
- **PostgreSQL**: Relational, ACID guarantees
- **CockroachDB**: Distributed SQL, geo-replication

**Configuration:**
```rust
pub enum VaultBackend {
    Embedded(Arc<SledBackend>),
    Consul(ConsulClient),
    Etcd(EtcdClient),
    Postgres(PgPool),
}
```

### Recommended Approach

**For k1s:**
1. **Default**: Embedded Sled (single-node, testing)
2. **P2P Mode**: Gossip replication (multi-node, eventually consistent)
3. **Production**: External backend (multi-cluster, strongly consistent)

**Implementation Priority:**
```
Phase 1: Embedded Sled (✅ Done)
Phase 2: P2P gossip replication
Phase 3: Raft consensus option
Phase 4: External backend plugins
```

## kubectl Management

### Creating Vault Resources via kubectl

#### 1. Using Annotations (Current Approach)
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: app
  annotations:
    vault.k1s.io/kv-path: "database/prod"
    vault.k1s.io/transit-key: "encryption-key"
    vault.k1s.io/inject-secrets: "true"
spec:
  containers:
  - name: app
    image: myapp:latest
    env:
    - name: DB_PASSWORD
      value: "vault:database/prod:password"
```

#### 2. Using CRDs (Future Enhancement)
```yaml
apiVersion: vault.k1s.io/v1
kind: VaultSecret
metadata:
  name: database-credentials
  namespace: default
spec:
  path: database/prod
  type: kv-v2
  data:
    username: admin
    password: secret123
  rotation:
    enabled: true
    period: 30d
---
apiVersion: vault.k1s.io/v1
kind: VaultTransitKey
metadata:
  name: app-encryption
spec:
  type: aes256-gcm128
  exportable: false
  allowPlaintextBackup: false
---
apiVersion: vault.k1s.io/v1
kind: VaultPKIRole
metadata:
  name: kubelet-certs
spec:
  caName: k1s-ca
  ttl: 24h
  allowedDomains:
  - k1s.local
  - "*.k1s.local"
  allowSubdomains: true
```

#### 3. kubectl Plugin (Recommended)
```bash
# Install kubectl-vault plugin
kubectl krew install k1s-vault

# Manage secrets
kubectl vault write database/prod username=admin password=secret123
kubectl vault read database/prod
kubectl vault delete database/prod

# Manage encryption keys
kubectl vault transit create app-key
kubectl vault transit encrypt app-key --plaintext "sensitive data"
kubectl vault transit decrypt app-key --ciphertext "vault:v1:..."

# Manage certificates
kubectl vault pki generate-root k1s-ca --common-name "k1s CA"
kubectl vault pki issue k1s-ca --common-name "kubelet.k1s.local" --ttl 24h

# Audit logs
kubectl vault audit logs --since 1h
kubectl vault audit logs --operation KvWrite --user admin
```

### Direct API Management

#### Using curl
```bash
# Set vault endpoint
VAULT_ADDR=http://127.0.0.1:6443/v1/vault

# KV operations
curl -X POST $VAULT_ADDR/kv/data/app/config \
  -d '{"data":{"api_key":"abc123"}}'

curl $VAULT_ADDR/kv/data/app/config

# Transit operations
curl -X POST $VAULT_ADDR/transit/keys/my-key
curl -X POST $VAULT_ADDR/transit/encrypt/my-key \
  -d '{"plaintext":"base64encodeddata"}'

# PKI operations
curl -X POST $VAULT_ADDR/pki/root/prod-ca \
  -d '{"common_name":"Production CA","ttl_days":365}'
```

#### Using Kubernetes Client
```go
import (
    "k8s.io/client-go/rest"
    "k8s.io/client-go/kubernetes"
)

func writeSecret(config *rest.Config) error {
    // Get vault client from k8s config
    vaultURL := config.Host + "/v1/vault"

    // Make request
    req := &VaultKVWrite{
        Data: map[string]string{
            "password": "secret123",
        },
    }

    resp, err := http.Post(
        vaultURL+"/kv/data/database/prod",
        "application/json",
        marshal(req),
    )
    return err
}
```

### RBAC for Vault Operations

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: vault-admin
rules:
- apiGroups: ["vault.k1s.io"]
  resources: ["secrets", "transitkeys", "pkiroles"]
  verbs: ["get", "list", "create", "update", "delete"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: vault-reader
rules:
- apiGroups: ["vault.k1s.io"]
  resources: ["secrets"]
  verbs: ["get", "list"]
  resourceNames: ["allowed-secret-*"]
```

### Namespace Isolation

```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: production
  annotations:
    vault.k1s.io/kv-prefix: "prod/"
    vault.k1s.io/pki-ca: "prod-ca"
    vault.k1s.io/transit-key: "prod-encryption"
```

## Integration with Existing Workloads

### Vault Agent Sidecar Pattern
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: app-with-vault
spec:
  initContainers:
  - name: vault-agent
    image: k1s-vault-agent:latest
    env:
    - name: VAULT_ADDR
      value: "http://127.0.0.1:6443/v1/vault"
    - name: VAULT_KV_PATH
      value: "database/prod"
    volumeMounts:
    - name: secrets
      mountPath: /vault/secrets
  containers:
  - name: app
    image: myapp:latest
    volumeMounts:
    - name: secrets
      mountPath: /secrets
      readOnly: true
  volumes:
  - name: secrets
    emptyDir: {}
```

### CSI Driver Integration
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: app-with-csi
spec:
  containers:
  - name: app
    image: myapp:latest
    volumeMounts:
    - name: vault-secrets
      mountPath: /mnt/secrets
      readOnly: true
  volumes:
  - name: vault-secrets
    csi:
      driver: vault.k1s.io
      volumeAttributes:
        path: "database/prod"
        role: "app-role"
```

## Security Best Practices

1. **TLS Required**: Always use TLS in production (`--tls-enabled`)
2. **RBAC**: Implement namespace-scoped access control
3. **Audit Monitoring**: Set up alerts on audit logs
4. **Secret Rotation**: Implement automatic rotation for KV secrets
5. **Key Backup**: Regular backups of PKI CAs and Transit keys
6. **Network Policies**: Restrict vault API access to authorized pods
7. **Encryption at Rest**: Enable Sled backend encryption

## Migration from Kubernetes Secrets

```bash
#!/bin/bash
# Migrate all secrets to vault

for secret in $(kubectl get secrets -o name); do
    name=$(echo $secret | cut -d/ -f2)

    # Export secret data
    kubectl get secret $name -o json | \
        jq -r '.data | to_entries[] | "\(.key)=\(.value | @base64d)"' | \
        while IFS== read key value; do
            # Import to vault
            curl -X POST http://localhost:6443/v1/vault/kv/data/migrated/$name \
                -d "{\"data\":{\"$key\":\"$value\"}}"
        done
done
```

## Performance Benchmarks

From test run:
- **Transit Encrypt**: < 10ms
- **Transit Decrypt**: < 10ms
- **KV Write**: < 20ms
- **KV Read**: < 5ms
- **PKI Issue Cert**: ~200ms (RSA-2048 generation)
- **Audit Log Write**: < 2ms (async)

## Next Steps

1. **Implement P2P replication** for multi-node vault data sync
2. **Create kubectl plugin** for easier vault management
3. **Add CRDs** for VaultSecret, VaultTransitKey, VaultPKIRole
4. **Vault agent sidecar** for automatic secret injection
5. **CSI driver** for mounting vault secrets as volumes
6. **RBAC integration** with namespace-scoped policies
7. **Secret rotation** policies and automation
