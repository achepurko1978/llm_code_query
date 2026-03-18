FROM ubuntu:latest

ENV DEBIAN_FRONTEND=noninteractive

# Install build essentials and tooling required to add the official LLVM apt repo.
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    gcc \
    g++ \
    gdb \
    make \
    cmake \
    ninja-build \
    pkg-config \
    git \
    curl \
    wget \
    gnupg \
    lsb-release \
    software-properties-common \
    ca-certificates \
    python3 \
    python3-pip \
    && rm -rf /var/lib/apt/lists/*

# Install the newest Clang/LLVM toolchain from apt.llvm.org.
RUN wget -O /tmp/llvm.sh https://apt.llvm.org/llvm.sh \
    && chmod +x /tmp/llvm.sh \
    && /tmp/llvm.sh 22 \
    && apt-get update && apt-get install -y --no-install-recommends \
    clang-22 \
    clangd-22 \
    clang-format-22 \
    clang-tidy-22 \
    clang-tools-22 \
    lldb-22 \
    lld-22 \
    llvm-22 \
    llvm-22-dev \
    libclang-22-dev \
    libc++-22-dev \
    libc++abi-22-dev \
    && ln -s /usr/bin/clang-22 /usr/bin/clang \
    && ln -s /usr/bin/clang++-22 /usr/bin/clang++ \
    && ln -s /usr/bin/clangd-22 /usr/bin/clangd \
    && rm -f /tmp/llvm.sh \
    && rm -rf /var/lib/apt/lists/*

# Install Rust toolchain (system packages for compatibility with PEP 668 managed Python).
RUN apt-get update && apt-get install -y --no-install-recommends \
    rustc \
    cargo \
    && rm -rf /var/lib/apt/lists/*

# Pre-build Rust dependencies so they are cached in the image.
COPY clang_mcp_rs/Cargo.toml clang_mcp_rs/Cargo.lock /workspace/clang_mcp_rs/
COPY clang_mcp_rs/.cargo /workspace/clang_mcp_rs/.cargo
RUN mkdir -p /workspace/clang_mcp_rs/src \
    && echo 'fn main() {}' > /workspace/clang_mcp_rs/src/main.rs \
    && cd /workspace/clang_mcp_rs \
    && cargo build --release 2>/dev/null || true \
    && cargo build --release --tests 2>/dev/null || true \
    && rm -rf src

WORKDIR /workspace

CMD ["bash"]
