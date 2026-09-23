# syntax=docker/dockerfile:1
# Rust image, pinned to one exact version so a rebuild in a month gives the
# same toolchain as a rebuild today. A newer patch than crates/osf's own
# rust-version = "1.85": globset 0.4.20, an exact-pinned dependency, needs
# rustc 1.88 or later to build.
FROM rust:1.98.1-bookworm

ARG GIT_TOWN_VERSION=23.0.3
# TARGETARCH is set by BuildKit to "amd64" or "arm64". It picks the right
# git-town download for the two common processor architectures.
ARG TARGETARCH

RUN apt-get update && apt-get install -y --no-install-recommends \
      git \
      ca-certificates \
      curl \
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

# moon runs every factory check; pinned by version and checksum. The
# release ships a tar.xz holding the binary alongside its docs, not a
# bare binary, so this extracts the one file it needs.
ARG MOON_VERSION=2.5.5
ARG MOON_SHA256=f397057a22c88ff9e4b28a4a2fbf01d99f4d9a4f35e5a42634255a9f509ccbd6
RUN set -eu; \
    curl -fsSL -o /tmp/moon.tar.xz \
      "https://github.com/moonrepo/moon/releases/download/v${MOON_VERSION}/moon_cli-x86_64-unknown-linux-gnu.tar.xz"; \
    echo "${MOON_SHA256}  /tmp/moon.tar.xz" | sha256sum -c -; \
    tar -xJf /tmp/moon.tar.xz -C /tmp moon_cli-x86_64-unknown-linux-gnu/moon; \
    install -o root -g root -m 0555 /tmp/moon_cli-x86_64-unknown-linux-gnu/moon /opt/factory/bin/moon; \
    rm -rf /tmp/moon.tar.xz /tmp/moon_cli-x86_64-unknown-linux-gnu; \
    /opt/factory/bin/moon --version

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

ENV PATH="/opt/factory/bin:${PATH}"

USER dev
WORKDIR /workspace
