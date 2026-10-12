# The Windows build: mingw-w64 cross-compilers for the engine (C, and C++
# for NetHack's std::regex) and Rust's x86_64-pc-windows-gnu for the
# extension, NSIS for the installer. Native gcc (and ncurses' headers,
# which NetHack's unix build includes) builds the engine for this machine
# first: its tools make the data files and the generated sources.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential libncurses-dev gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 \
        nsis make git patch curl ca-certificates xz-utils bzip2 zip unzip \
    && rm -rf /var/lib/apt/lists/*
ARG RUST_VERSION=1.97.1
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
        --default-toolchain $RUST_VERSION --target x86_64-pc-windows-gnu \
    && chmod -R a+w /opt/cargo /opt/rustup
