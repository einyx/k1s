# k1s Secrets Encryption

## Implemented: Encryption at Rest

**Status**: ✅ Complete (not yet enabled)

### How It Works

1. **AES-256-GCM Encryption**
   - Automatic encryption for all Secret resources
   - Key stored in `.k1s/pki/secrets-key.enc` (600 permissions)
   - Transparent to API clients

2. **Key Generation**
   - Auto-generated on first startup
   - 32-byte random key via OsRng
   - Persisted to disk securely

3. **Storage**
   - Secrets encrypted before Sled storage
   - Format: base64(nonce || ciphertext)
   - Nonce: 12 bytes (unique per encryption)

### Enabling Encryption

In `k1s-api-server/src/server.rs`:

```rust
use k1s_storage::SecretEncryption;

// After opening storage:
let encryption = SecretEncryption::load_or_generate(
    &config.data_dir.join("pki/secrets-key.enc")
)?;
let storage = Arc::new(storage.with_encryption(encryption));
```

## Future Enhancements

### 1. Audit Logging

**Requirements** (from user: "audit and rotation /scope"):

```rust
// Audit event structure
struct SecretAuditEvent {
    timestamp: DateTime<Utc>,
    user: String,           // Who accessed
    action: SecretAction,   // Read/Write/Delete
    secret: String,         // Which secret (namespace/name)
    result: AuditResult,    // Success/Denied
    source_ip: String,      // Where from
}

enum SecretAction {
    Read,
    Create,
    Update,
    Delete,
    Encrypt,
    Decrypt,
}
```

**Implementation**:
- Append-only audit log in separate Sled tree
- Structured JSON events
- Queryable via API: `/api/v1/audit/secrets`
- Retention policy (default: 90 days)

### 2. Key Rotation

**Requirements** (from user: "audit and rotation /scope"):

```rust
// Multi-version key support
struct EncryptionKeyRing {
    current: SecretEncryption,    // Active key (v2)
    previous: Vec<SecretEncryption>, // Old keys (v1, v0...)
}

// Rotation process
async fn rotate_encryption_key() {
    1. Generate new key (v_n+1)
    2. Re-encrypt all secrets with new key
    3. Mark old key as previous
    4. Audit log: KeyRotated event
}
```

**Triggers**:
- Manual: `k1s secrets rotate-key`
- Scheduled: Every 90 days
- Compliance: On key compromise

### 3. Scope / Secret Backends

**Requirements** (from user: "audit and rotation /scope"):

Multiple backend support via CRD:

```yaml
apiVersion: secrets.k1s.io/v1
kind: SecretStore
metadata:
  name: vault-backend
spec:
  provider: vault
  vault:
    address: http://vault:8200
    path: secret/data/k1s
    authMethod: kubernetes
    role: k1s-secrets
```

**Backends**:
1. **Local** (default): Encrypted Sled
2. **Vault**: HashiCorp Vault transit/kv
3. **Cloud KMS**: AWS/GCP/Azure key management
4. **Plugin**: Custom via WASM/gRPC

**Scope Options**:
- Namespace-scoped: Different backends per namespace
- Global: One backend for all secrets
- Hybrid: Fallback chain (Vault -> Local)

## Security Best Practices

### Current
- ✅ Encryption at rest (AES-256-GCM)
- ✅ Secure key storage (file perms 600)
- ✅ Random nonce per encryption
- ✅ Authenticated encryption (GCM)

### Recommended Additions
- 🔄 Key rotation policy
- 🔄 Audit logging
- 🔄 External key management (KMS/TPM)
- 🔄 Mutual TLS for API access
- 🔄 RBAC for secret access
- 🔄 Secret scanning (no plaintext in logs)

## Migration Path

1. **Phase 1** (Current): Encryption at rest ✅
2. **Phase 2**: Enable in API server
3. **Phase 3**: Audit logging
4. **Phase 4**: Key rotation
5. **Phase 5**: External backends (Vault/KMS)
6. **Phase 6**: CRD-based SecretStore

## References

- AES-GCM: NIST SP 800-38D
- Key Management: NIST SP 800-57
- Kubernetes Secrets: https://kubernetes.io/docs/concepts/configuration/secret/
- Vault Integration: https://www.vaultproject.io/docs/platform/k8s
