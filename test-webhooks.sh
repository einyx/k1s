#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== k1s Admission Webhook Test ===${NC}\n"

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    if [ ! -z "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
    fi
    rm -rf .k1s-test 2>/dev/null || true
}

trap cleanup EXIT

# Create test directory
TEST_DIR=".k1s-test"
rm -rf $TEST_DIR
mkdir -p $TEST_DIR

echo -e "${BLUE}Step 1: Starting k1s server${NC}"
./target/release/k1s server \
    --data-dir=$TEST_DIR \
    --bind-address=127.0.0.1:6443 \
    > $TEST_DIR/server.log 2>&1 &

SERVER_PID=$!
echo "Server PID: $SERVER_PID"

# Wait for server to start
echo -e "${BLUE}Step 2: Waiting for server to be ready...${NC}"
for i in {1..30}; do
    if curl -k https://127.0.0.1:6443/healthz > /dev/null 2>&1; then
        echo -e "${GREEN}✓ Server is ready${NC}"
        break
    fi
    if [ $i -eq 30 ]; then
        echo -e "${RED}✗ Server failed to start${NC}"
        cat $TEST_DIR/server.log
        exit 1
    fi
    sleep 1
    echo -n "."
done

# Extract kubeconfig
echo -e "\n${BLUE}Step 3: Configuring kubectl${NC}"
export KUBECONFIG=$TEST_DIR/k1s.yaml
# Wait for kubeconfig to be generated
for i in {1..10}; do
    if [ -f $KUBECONFIG ]; then
        echo -e "${GREEN}✓ Kubeconfig found${NC}"
        break
    fi
    sleep 1
done
if [ ! -f $KUBECONFIG ]; then
    echo -e "${RED}✗ Kubeconfig not found${NC}"
    exit 1
fi

# Test kubectl connection
echo -e "\n${BLUE}Step 4: Testing kubectl connection${NC}"
kubectl version --short 2>/dev/null || kubectl version
echo -e "${GREEN}✓ Connected to k1s${NC}"

# Run tests
echo -e "\n${BLUE}=== Running Webhook Tests ===${NC}\n"

# Test 1: Valid pod (should succeed and be mutated)
echo -e "${BLUE}Test 1: Valid pod (should succeed with mutations)${NC}"
if kubectl apply -f test-manifests/01-valid-pod.yaml > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Pod created successfully${NC}"
    kubectl get pod nginx-test -o yaml | grep -A 5 "labels:\|annotations:\|resources:" || true
    kubectl delete pod nginx-test --wait=false > /dev/null 2>&1
else
    echo -e "${RED}✗ Failed to create valid pod${NC}"
fi

# Test 2: Invalid pod - no containers (should fail)
echo -e "\n${BLUE}Test 2: Invalid pod - no containers (should fail)${NC}"
if kubectl apply -f test-manifests/02-invalid-pod-no-containers.yaml 2>&1 | grep -q "must have at least one container"; then
    echo -e "${GREEN}✓ Correctly rejected: No containers${NC}"
else
    echo -e "${RED}✗ Should have rejected pod with no containers${NC}"
fi

# Test 3: Invalid pod - bad name (should fail)
echo -e "\n${BLUE}Test 3: Invalid pod - invalid name (should fail)${NC}"
if kubectl apply -f test-manifests/03-invalid-pod-bad-name.yaml 2>&1 | grep -q "lowercase alphanumeric"; then
    echo -e "${GREEN}✓ Correctly rejected: Invalid name format${NC}"
else
    echo -e "${RED}✗ Should have rejected pod with invalid name${NC}"
fi

# Test 4: Pod with latest tag (should succeed with warning)
echo -e "\n${BLUE}Test 4: Pod with latest tag (should succeed)${NC}"
if kubectl apply -f test-manifests/04-pod-latest-tag.yaml > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Pod created (webhook may have warned about latest tag)${NC}"
    kubectl delete pod nginx-latest --wait=false > /dev/null 2>&1
else
    echo -e "${RED}✗ Failed to create pod with latest tag${NC}"
fi

# Test 5: Valid deployment (should succeed and be mutated)
echo -e "\n${BLUE}Test 5: Valid deployment (should succeed with mutations)${NC}"
if kubectl apply -f test-manifests/05-valid-deployment.yaml > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Deployment created successfully${NC}"
    kubectl get deployment nginx-deployment -o yaml | grep -A 3 "replicas:\|strategy:" || true
    kubectl delete deployment nginx-deployment --wait=false > /dev/null 2>&1
else
    echo -e "${RED}✗ Failed to create valid deployment${NC}"
fi

# Test 6: Invalid deployment - label mismatch (should fail)
echo -e "\n${BLUE}Test 6: Invalid deployment - selector mismatch (should fail)${NC}"
if kubectl apply -f test-manifests/06-invalid-deployment-mismatch.yaml 2>&1 | grep -q "not found in pod template"; then
    echo -e "${GREEN}✓ Correctly rejected: Selector label mismatch${NC}"
else
    echo -e "${RED}✗ Should have rejected deployment with mismatched labels${NC}"
fi

echo -e "\n${BLUE}=== Test Summary ===${NC}"
echo -e "${GREEN}Webhook validation and mutation tests completed${NC}"
echo -e "\nCheck server logs for detailed webhook activity:"
echo -e "${YELLOW}tail -f $TEST_DIR/server.log${NC}"
