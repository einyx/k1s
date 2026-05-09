# OpenAPI Client-Side Validation

## Overview

k1s now supports client-side validation through comprehensive OpenAPI v2 schemas. kubectl can fetch these schemas and validate resources before sending them to the server.

## Implementation

### OpenAPI v2 Endpoint

**Endpoint**: `/openapi/v2`
**Format**: Swagger 2.0 / OpenAPI 2.0

### Supported Resources

#### Core v1 (group: "")
- Pod
- Namespace
- Node
- Service
- ConfigMap
- Secret
- Endpoints
- PersistentVolume
- PersistentVolumeClaim
- ServiceAccount
- Event

#### Apps v1 (group: "apps")
- Deployment
- ReplicaSet
- DaemonSet
- StatefulSet

#### Batch v1 (group: "batch")
- Job
- CronJob

#### Storage v1 (group: "storage.k8s.io")
- StorageClass

#### RBAC v1 (group: "rbac.authorization.k8s.io")
- Role
- RoleBinding
- ClusterRole
- ClusterRoleBinding

### Schema Features

Each resource schema includes:
- **x-kubernetes-group-version-kind**: Proper GVK annotations
- **required fields**: Validation of required fields
- **type definitions**: Strong typing for all properties
- **enums**: Enumerated values where applicable
- **$ref**: References to common types (ObjectMeta, LabelSelector, etc.)

### Common Types

- `io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta` - Standard metadata
- `io.k8s.apimachinery.pkg.apis.meta.v1.LabelSelector` - Label selectors
- `io.k8s.api.core.v1.PodTemplateSpec` - Pod templates
- `io.k8s.api.core.v1.PodSpec` - Pod specifications
- `io.k8s.api.core.v1.Container` - Container definitions

## Usage

### Automatic Client-Side Validation

kubectl automatically uses the OpenAPI schemas for validation:

```bash
# This will validate against the schema before sending to server
kubectl apply -f pod.yaml
```

### Fetching Schemas

```bash
# View the full OpenAPI schema
kubectl proxy &
curl http://localhost:8001/openapi/v2

# Or directly from k1s
curl -k https://localhost:6443/openapi/v2
```

### Schema Validation Rules

#### Pod
- **Required**: `spec.containers` (at least one)
- **Container Required**: `name`, `image`
- **RestartPolicy**: `Always`, `OnFailure`, or `Never`

#### Deployment
- **Required**: `spec.selector`, `spec.template`
- **Replicas**: Must be >= 0

#### Job
- **Required**: `spec.template`
- **Parallelism/Completions**: Must be >= 0

#### CronJob
- **Required**: `spec.schedule`, `spec.jobTemplate`
- **ConcurrencyPolicy**: `Allow`, `Forbid`, or `Replace`

#### PersistentVolumeClaim
- **AccessModes**: `ReadWriteOnce`, `ReadOnlyMany`, or `ReadWriteMany`

#### RBAC
- **RoleBinding**: Requires `roleRef` with `apiGroup`, `kind`, `name`

## Examples

### Valid Pod (passes validation)
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: nginx
spec:
  containers:
    - name: nginx
      image: nginx:1.21
```

### Invalid Pod (fails validation)
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: nginx
spec:
  containers: []  # ERROR: At least one container required
```

### Valid Deployment
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nginx
spec:
  selector:
    matchLabels:
      app: nginx
  template:
    metadata:
      labels:
        app: nginx
    spec:
      containers:
        - name: nginx
          image: nginx:1.21
```

### Invalid Deployment
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nginx
spec:
  replicas: -1  # ERROR: Must be >= 0
  selector:
    matchLabels:
      app: nginx
  template:
    metadata:
      labels:
        app: redis  # ERROR: Doesn't match selector
    spec:
      containers:
        - name: nginx
          image: nginx:1.21
```

## OpenAPI v3

**Endpoint**: `/openapi/v3`
**Status**: Stub implementation (returns minimal schema)

OpenAPI v3 support can be added in the future for more advanced validation features.

## Testing

### Test client-side validation
```bash
# Start k1s server
k1s server

# Apply valid resource (should succeed)
kubectl apply -f valid-pod.yaml

# Apply invalid resource (should fail validation)
kubectl apply -f invalid-pod.yaml
```

### Disable validation (bypass schema check)
```bash
kubectl apply -f resource.yaml --validate=false
```

## Benefits

1. **Faster Feedback**: Errors caught client-side before server processing
2. **Reduced Server Load**: Invalid requests rejected before network transmission
3. **Better UX**: Clear validation errors with schema context
4. **IDE Support**: Editors can use schemas for autocomplete and validation
5. **kubectl Compatibility**: Full compatibility with kubectl's validation features

## Future Enhancements

- [ ] OpenAPI v3 full implementation
- [ ] CRD schema generation (when CRD support is added)
- [ ] Conversion webhook schemas
- [ ] Enhanced validation rules (regex patterns, custom validators)
- [ ] Schema caching and versioning
- [ ] Subresource schemas (status, scale, etc.)

## References

- [Kubernetes OpenAPI Spec](https://kubernetes.io/docs/concepts/overview/kubernetes-api/#openapi-and-swagger-definitions)
- [OpenAPI v2 Specification](https://swagger.io/specification/v2/)
- [kubectl Validation](https://kubernetes.io/docs/reference/generated/kubectl/kubectl-commands#apply)
