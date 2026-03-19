FROM ubuntu:latest

ENV DEBIAN_FRONTEND=noninteractive
ARG GIT_REPO_URL=https://github.com/achepurko1978/llm_code_query.git
ARG GIT_REF=main
ARG GIT_USER_NAME="Andrey Chepurko"
ARG GIT_USER_EMAIL=achepurko1978@users.noreply.github.com

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
    python-is-python3 \
    python3-clang \
    && rm -rf /var/lib/apt/lists/*

# Install Python MCP dependencies globally (no virtualenv).
RUN python -m pip install --break-system-packages \
    mcp \
    pytest

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

# Clone repository from GitHub directly at build time.
RUN git clone --filter=blob:none "$GIT_REPO_URL" /workspace \
    && cd /workspace \
    && git checkout "$GIT_REF" \
    && git submodule update --init --recursive

# Set default git identity inside the container to avoid exposing personal emails.
RUN git config --global user.name "$GIT_USER_NAME" \
    && git config --global user.email "$GIT_USER_EMAIL"

# Optionally pre-build Rust target if the Rust backend exists in the cloned repo.
RUN if [ -f /workspace/clang_mcp_rs/Cargo.toml ]; then \
        cd /workspace/clang_mcp_rs \
        && cargo build --release || true \
        && cargo build --release --tests || true; \
    fi

WORKDIR /workspace

CMD ["bash"]
