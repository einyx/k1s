# Build stage
FROM rust:1.75-alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig

WORKDIR /build

# Copy workspace
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary
RUN cargo build --release --package k1s-cli

# Runtime stage
FROM alpine:3.19

RUN apk add --no-cache \
    iptables \
    ip6tables \
    cni-plugins \
    containerd \
    containerd-ctr \
    ca-certificates \
    && mkdir -p /var/lib/k1s \
    && mkdir -p /etc/cni/net.d \
    && mkdir -p /opt/cni/bin

# Copy CNI plugins
RUN ln -sf /usr/libexec/cni/* /opt/cni/bin/

# Copy binary
COPY --from=builder /build/target/release/k1s /usr/local/bin/k1s

# Default CNI configuration
RUN echo '{"cniVersion":"1.0.0","name":"k1s","plugins":[{"type":"bridge","bridge":"k1s0","isGateway":true,"ipMasq":true,"ipam":{"type":"host-local","subnet":"10.42.0.0/24","routes":[{"dst":"0.0.0.0/0"}]}}]}' > /etc/cni/net.d/10-k1s.conflist

EXPOSE 6443 10250 4001

VOLUME ["/var/lib/k1s"]

ENTRYPOINT ["/usr/local/bin/k1s"]
CMD ["server"]
