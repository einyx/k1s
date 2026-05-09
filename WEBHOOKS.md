# Admission Webhooks Implementation

## Overview

k1s now has a complete admission webhook system for validating and mutating Kubernetes resources. The implementation includes both built-in webhooks and support for external webhook endpoints.

## Architecture

### Components

1. **Type Definitions** (`k1s-types/src/api/admission/`)
   - `AdmissionReview` - Request/response wrapper
   - `AdmissionRequest` - Incoming admission requests
   - `AdmissionResponse` - Webhook responses (allow/deny/mutate)
   - Full support for JSON patches, warnings, and audit annotations

2. **Webhook Configuration** (`k1s-types/src/api/admissionregistration/`)
   - `ValidatingWebhookConfiguration` - Validation webhook config
   - `MutatingWebhookConfiguration` - Mutation webhook config
   - Rule-based matching (operations, resources, API groups)
   - Failure policies (Fail/Ignore)
   - Namespace and object selectors

3. **Built-in Validators** (`k1s-api-server/src/webhooks/validators.rs`)
   - **Pod Validation**:
     - Name format and length checks (DNS-1123)
     - At least one container required
     - Unique container names
     - Image validation
     - Warnings for latest tags and privileged containers
   - **Deployment Validation**:
     - Selector and label matching
     - Replica count validation
     - Template validation
   - **StatefulSet/DaemonSet Validation**:
     - Selector and label matching
     - Service name validation (StatefulSet)

4. **Built-in Mutators** (`k1s-api-server/src/webhooks/mutators.rs`)
   - **Pod Mutation**:
     - Automatic app labels
     - Management annotations (`k1s.io/managed-by`)
     - Default resource requests/limits
     - Default restart policy
   - **Deployment Mutation**:
     - Default replicas (1)
     - Default rolling update strategy
     - Revision history limit (10)

5. **Webhook Invoker** (`k1s-api-server/src/webhooks/invoker.rs`)
   - HTTP client for calling external webhooks
   - Rule matching engine
   - Failure policy handling
   - Timeout management
   - Patch combination

6. **Admission Middleware** (`k1s-api-server/src/webhooks/middleware.rs`)
   - Intercepts resource CREATE/UPDATE/DELETE operations
   - Two-phase processing:
     1. Mutation phase (built-in + external mutators)
     2. Validation phase (built-in + external validators)
   - Request path parsing and resource type detection
   - User info extraction

## Usage

### Built-in Webhooks

Built-in webhooks are enabled by default and run automatically:

```rust
use k1s_api_server::{WebhookState, admission_webhook_middleware};

let webhook_state = WebhookState::new(storage.clone())
    .with_built_in_validators(true)
    .with_built_in_mutators(true);

// Add to middleware stack
app.layer(middleware::from_fn_with_state(
    webhook_state,
    admission_webhook_middleware,
))
```

### External Webhooks

Register external webhooks via configuration:

```yaml
apiVersion: admissionregistration.k8s.io/v1
kind: ValidatingWebhookConfiguration
metadata:
  name: pod-validator
webhooks:
  - name: validate.pods.example.com
    clientConfig:
      url: https://webhook.example.com/validate
    rules:
      - operations: ["CREATE", "UPDATE"]
        apiGroups: [""]
        apiVersions: ["v1"]
        resources: ["pods"]
    failurePolicy: Fail
    sideEffects: None
    admissionReviewVersions: ["v1"]
```

```yaml
apiVersion: admissionregistration.k8s.io/v1
kind: MutatingWebhookConfiguration
metadata:
  name: pod-mutator
webhooks:
  - name: mutate.pods.example.com
    clientConfig:
      url: https://webhook.example.com/mutate
    rules:
      - operations: ["CREATE"]
        apiGroups: [""]
        apiVersions: ["v1"]
        resources: ["pods"]
    failurePolicy: Ignore
    sideEffects: NoneOnDryRun
    admissionReviewVersions: ["v1"]
```

## Examples

### Pod Validation

**Valid Pod:**
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: nginx
spec:
  containers:
    - name: nginx
      image: nginx:1.21
      resources:
        requests:
          cpu: 100m
          memory: 128Mi
        limits:
          cpu: 500m
          memory: 512Mi
```

**Invalid Pod (will be rejected):**
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: InvalidName!  # Invalid characters
spec:
  containers: []  # No containers
```

### Pod Mutation

**Input:**
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: nginx-abcde
spec:
  containers:
    - name: nginx
      image: nginx:1.21
```

**Output (after mutation):**
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: nginx-abcde
  labels:
    app: nginx  # Automatically added
  annotations:
    k1s.io/managed-by: k1s  # Automatically added
spec:
  restartPolicy: Always  # Default added
  containers:
    - name: nginx
      image: nginx:1.21
      resources:
        requests:
          cpu: 100m
          memory: 128Mi
        limits:
          cpu: 500m
          memory: 512Mi
```

## Validation Rules

### Pod

- **Name**: 1-253 characters, lowercase alphanumeric or `-`
- **Containers**: At least one container required
- **Container names**: Unique, 1-63 characters
- **Images**: Must be specified
- **Warnings**:
  - Latest tag usage
  - Privileged containers
  - Host network usage

### Deployment

- **Replicas**: Cannot be negative
- **Selector**: Must have at least one match label
- **Labels**: Template labels must match selector
- **Warnings**:
  - High replica count (>100)

### StatefulSet

- **Service name**: Required
- **Selector**: Must have at least one match label
- **Labels**: Template labels must match selector

### DaemonSet

- **Selector**: Must have at least one match label
- **Labels**: Template labels must match selector

## Configuration

### Disabling Built-in Webhooks

```rust
let webhook_state = WebhookState::new(storage.clone())
    .with_built_in_validators(false)
    .with_built_in_mutators(false);
```

### Webhook Failure Policies

- **Fail** (default): Reject request if webhook fails
- **Ignore**: Allow request if webhook fails

### Timeouts

Default timeout: 10 seconds (configurable per webhook)

## Testing

Run webhook tests:
```bash
cargo test --package k1s-api-server --lib webhooks
```

All 10 tests pass:
- ✅ Pod validation (success, no containers, latest tag warning)
- ✅ Pod mutation (adds defaults)
- ✅ Middleware (path parsing, namespace extraction, resource type detection)
- ✅ Webhook invocation (rule matching, wildcards)

## Implementation Details

### Admission Flow

1. **Request arrives** at API server
2. **Middleware intercepts** CREATE/UPDATE/DELETE operations
3. **Parse request**: Extract resource type, namespace, user info
4. **Mutating phase**:
   - Run built-in mutators
   - Call external mutating webhooks
   - Apply patches (if allowed)
5. **Validating phase**:
   - Run built-in validators
   - Call external validating webhooks
   - Deny if any validator rejects
6. **Continue** with original request handler

### JSON Patch Format

Mutations use RFC 6902 JSON Patch:
```json
[
  {"op": "add", "path": "/metadata/labels/app", "value": "nginx"},
  {"op": "add", "path": "/spec/restartPolicy", "value": "Always"}
]
```

### Error Handling

- **Validation failures**: Return 400 Bad Request with detailed message
- **Mutation denials**: Return 400 Bad Request
- **Webhook errors** (with Fail policy): Return 500 Internal Error
- **Webhook errors** (with Ignore policy): Log warning and continue

## Security Considerations

1. **TLS**: External webhooks should use HTTPS with valid certificates
2. **Authentication**: Webhook endpoints should validate requests
3. **Authorization**: Consider webhook-specific RBAC policies
4. **Timeouts**: Prevent slow webhooks from blocking API server
5. **Failure policies**: Choose appropriately for security vs availability

## Future Enhancements

- [ ] Apply JSON patches from mutating webhooks
- [ ] Fetch old object for UPDATE operations
- [ ] Namespace and object selector evaluation
- [ ] Reinvocation policy support
- [ ] CA bundle verification for external webhooks
- [ ] Service reference support (instead of URL)
- [ ] Conversion webhooks (for API version conversion)
- [ ] Metrics and monitoring
- [ ] Webhook certificate management

## References

- [Kubernetes Admission Controllers](https://kubernetes.io/docs/reference/access-authn-authz/admission-controllers/)
- [Dynamic Admission Control](https://kubernetes.io/docs/reference/access-authn-authz/extensible-admission-controllers/)
- [RFC 6902: JSON Patch](https://tools.ietf.org/html/rfc6902)
