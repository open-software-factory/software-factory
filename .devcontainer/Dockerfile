# syntax=docker/dockerfile:1
# Rust image, pinned to one exact version so a rebuild in a month gives the
# same toolchain as a rebuild today. A newer patch than crates/osf's own
# rust-version = "1.85": globset 0.4.20, an exact-pinned dependency, needs
# rustc 1.88 or later to build.
FROM rust:1.98.1-bookworm

ARG GIT_TOWN_VERSION=23.0.3
# TARGETARCH is set by BuildKit to "amd64" or "arm64". It picks the right
# git-town, moon and Node download for the two common processor architectures.
ARG TARGETARCH

# moon, pinned to the version the checkpoint runner needs. Checksums are the
# official SHA-256 published next to each release asset.
ARG MOON_VERSION=2.5.5
ARG MOON_SHA256_AMD64=f397057a22c88ff9e4b28a4a2fbf01d99f4d9a4f35e5a42634255a9f509ccbd6
ARG MOON_SHA256_ARM64=39d55e88774677181d1af3566c4ccd2994cd2d9d8199fcf30dc2d6defc05ac78

# Node 22, matched to the version this repo's package.json pins pnpm against.
ARG NODE_VERSION=22.23.3
ARG NODE_SHA256_AMD64=df450af89261115ef9f9e3830c3eeb2cc9213b63c720b1af623cb5dcbe2e02de
ARG NODE_SHA256_ARM64=a44aeb94849a299b22df10b9e622ec2f605c2183501bc40590705131de7c740f

# pnpm, must match this repo's root package.json "packageManager" field.
ARG PNPM_VERSION=10.14.0

RUN apt-get update && apt-get install -y --no-install-recommends \
      git \
      ca-certificates \
      curl \
      xz-utils \
    && rm -rf /var/lib/apt/lists/*

# git-town, matched to the build's processor architecture.
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) GIT_TOWN_ARCH=intel_64 ;; \
      arm64) GIT_TOWN_ARCH=arm_64 ;; \
      *) echo "git-town: unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/git-town.tar.gz \
      "https://github.com/git-town/git-town/releases/download/v${GIT_TOWN_VERSION}/git-town_linux_${GIT_TOWN_ARCH}.tar.gz"; \
    tar -xzf /tmp/git-town.tar.gz -C /tmp git-town; \
    install -o root -g root -m 0755 /tmp/git-town /usr/local/bin/git-town; \
    rm -f /tmp/git-town.tar.gz /tmp/git-town; \
    git-town --version

# moon, the task runner. moonrepo publishes a standalone binary per target
# in each GitHub release, so no proto bootstrap step is needed here.
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) MOON_TARGET=x86_64-unknown-linux-gnu; MOON_SHA256="${MOON_SHA256_AMD64}" ;; \
      arm64) MOON_TARGET=aarch64-unknown-linux-gnu; MOON_SHA256="${MOON_SHA256_ARM64}" ;; \
      *) echo "moon: unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/moon.tar.xz \
      "https://github.com/moonrepo/moon/releases/download/v${MOON_VERSION}/moon_cli-${MOON_TARGET}.tar.xz"; \
    echo "${MOON_SHA256}  /tmp/moon.tar.xz" | sha256sum -c -; \
    tar -xJf /tmp/moon.tar.xz -C /tmp "moon_cli-${MOON_TARGET}/moon"; \
    install -o root -g root -m 0755 "/tmp/moon_cli-${MOON_TARGET}/moon" /usr/local/bin/moon; \
    rm -rf /tmp/moon.tar.xz "/tmp/moon_cli-${MOON_TARGET}"; \
    moon --version

# Node, from the official nodejs.org binary tarball, checked against its
# published SHA-256. This avoids adding a NodeSource apt repository and
# avoids Debian's own nodejs package, whose bookworm version trails 22 and
# is not pinned to one exact patch across rebuilds.
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) NODE_TARGET=linux-x64; NODE_SHA256="${NODE_SHA256_AMD64}" ;; \
      arm64) NODE_TARGET=linux-arm64; NODE_SHA256="${NODE_SHA256_ARM64}" ;; \
      *) echo "node: unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/node.tar.xz \
      "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-${NODE_TARGET}.tar.xz"; \
    echo "${NODE_SHA256}  /tmp/node.tar.xz" | sha256sum -c -; \
    mkdir -p /usr/local/lib/nodejs; \
    tar -xJf /tmp/node.tar.xz -C /usr/local/lib/nodejs; \
    mv "/usr/local/lib/nodejs/node-v${NODE_VERSION}-${NODE_TARGET}" /usr/local/lib/nodejs/node; \
    chown -R root:root /usr/local/lib/nodejs/node; \
    rm -f /tmp/node.tar.xz; \
    /usr/local/lib/nodejs/node/bin/node --version

ENV PATH="/usr/local/lib/nodejs/node/bin:${PATH}"
ENV COREPACK_HOME=/usr/local/share/corepack

# pnpm, enabled and pinned through corepack rather than installed loose, so
# the version in use always matches PNPM_VERSION above.
RUN corepack enable && \
    corepack prepare "pnpm@${PNPM_VERSION}" --activate && \
    chmod -R a+rX "${COREPACK_HOME}" && \
    pnpm --version

# Build osf from this workspace. The build directory sits outside
# /workspace, the path the running container later bind-mounts the repo
# onto, so the installed binary below does not depend on that mount.
WORKDIR /usr/src/osf-build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --bin osf

# Install the binary root-owned and read-only for every user. The
# container user, created below, owns neither the file nor its directory,
# so it cannot replace osf with a version that skips a check.
RUN mkdir -p /opt/factory/bin && \
    install -o root -g root -m 0555 /usr/src/osf-build/target/release/osf /opt/factory/bin/osf && \
    rm -rf /usr/src/osf-build

# Root-owned git hooks. Each file only calls osf; see docs/development.md
# for what each check does and how to run it by hand.
COPY .devcontainer/githooks/pre-commit /opt/factory/githooks/pre-commit
COPY .devcontainer/githooks/commit-msg /opt/factory/githooks/commit-msg
COPY .devcontainer/githooks/pre-push /opt/factory/githooks/pre-push
RUN chown root:root /opt/factory/githooks /opt/factory/githooks/pre-commit \
      /opt/factory/githooks/commit-msg /opt/factory/githooks/pre-push && \
    chmod 0555 /opt/factory/githooks/pre-commit /opt/factory/githooks/commit-msg \
      /opt/factory/githooks/pre-push

# core.hooksPath goes in the SYSTEM git config (/etc/gitconfig), not the
# repository's or the user's own config. The container user cannot repoint
# it, because it does not own that file.
RUN git config --system core.hooksPath /opt/factory/githooks

# The git wrapper: refuses --no-verify and -n on commit, and --no-verify
# on push, then calls the real git. Root-owned and read-only for the same
# reason as osf above. docs/development.md states plainly what it cannot do.
COPY .devcontainer/git-wrapper /opt/factory/bin/git
RUN chown root:root /opt/factory/bin/git && chmod 0555 /opt/factory/bin/git

# Non-root container user, with no password-less root access: no entry
# for it in /etc/sudoers, and it is not in the sudo group.
RUN useradd --create-home --shell /bin/bash dev

# The toolchain under /usr/local/cargo and /usr/local/rustup stays
# root-owned, so dev cannot replace rustc or cargo. dev gets its own
# writable CARGO_HOME for its registry cache and config; the cargo/rustc
# binaries are still found via /usr/local/cargo/bin, already on PATH.
ENV CARGO_HOME=/home/dev/.cargo

# The base image ships this toolchain group- and other-writable, on both
# directories and many of the binaries and libraries themselves, so dev
# could still replace or edit rustc or cargo in place. Strip only the
# write bits; nothing here needs to be executable-only or unreadable.
RUN find /usr/local/cargo /usr/local/rustup -perm /022 -exec chmod go-w {} +

ENV PATH="/opt/factory/bin:${PATH}"

USER dev
WORKDIR /workspace
