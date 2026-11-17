# Modified by ReallyMe Technologies Limited.
# Container build for the ReallyMe protoc plugin.
#
# Builds a minimal image suitable for pushing to the Buf Schema Registry
# as a custom remote plugin.
#
# Build (must target linux/amd64 for BSR compatibility):
#   docker build --platform linux/amd64 -t reallyme-protoc-gen-buffa -f protoc-gen-buffa/Dockerfile .
#
# Optional BSR publication requires a separately approved registry target:
#   buf beta registry plugin push --visibility public --image reallyme-protoc-gen-buffa
#
# Test locally:
#   echo '<CodeGeneratorRequest bytes>' | docker run -i reallyme-protoc-gen-buffa

FROM rust:1.95.0-slim AS builder

RUN apt-get update && apt-get install -y pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace files needed for the build.
COPY Cargo.toml Cargo.lock ./
COPY buffa/           buffa/
COPY buffa-build/     buffa-build/
COPY buffa-proto-options/ buffa-proto-options/
COPY buffa-remote-derive/ buffa-remote-derive/
COPY buffa-yaml/ buffa-yaml/
COPY examples/buffa-smolstr/ examples/buffa-smolstr/
COPY buffa-codegen/   buffa-codegen/
COPY buffa-descriptor/ buffa-descriptor/
COPY buffa-test/      buffa-test/
COPY buffa-types/     buffa-types/
COPY protoc-gen-buffa/ protoc-gen-buffa/
COPY protoc-gen-buffa-packaging/ protoc-gen-buffa-packaging/

RUN cargo build --release --locked --bin protoc-gen-reallyme-buffa

# Minimal runtime image — no OS, no shell, just the binary.
FROM scratch
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /build/target/release/protoc-gen-reallyme-buffa /protoc-gen-reallyme-buffa
USER nobody
ENTRYPOINT ["/protoc-gen-reallyme-buffa"]
