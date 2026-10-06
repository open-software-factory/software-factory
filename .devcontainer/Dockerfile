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

# The one folder the git wrapper treats as "the workspace": devcontainer.json
# binds the repository here, and OSF_WORKSPACE_ROOT (below) is how the
# wrapper reads the same value, so the two files never name it separately.
ARG WORKSPACE_ROOT=/workspace

# The container user's home, named once here so the omp and dsh hook
# directories below reach it through this build argument rather than
# repeating the literal path.
ARG DEV_HOME=/home/dev

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

# GitHub CLI, for the builder role: opening and checking pull requests.
ARG GH_VERSION=2.102.0
ARG GH_SHA256_AMD64=bb766f710eef8ede859c18578c72c327597cd4c8a85b06001b1f3843c6019386
ARG GH_SHA256_ARM64=7862c86c72f43df3a2d93ddde6f473285b4e2af61b494849846827e513ef6484

# actionlint, for the builder role: the writing-a-workflow skill and this
# project's own CI both lint GitHub Actions workflow files with it.
ARG ACTIONLINT_VERSION=1.7.12
ARG ACTIONLINT_SHA256_AMD64=8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8
ARG ACTIONLINT_SHA256_ARM64=325e971b6ba9bfa504672e29be93c24981eeb1c07576d730e9f7c8805afff0c6

# uv, Astral's Python tool installer. Needed only to install SkillSpector
# and SkillEvaluator below; this image does not otherwise run Python.
ARG UV_VERSION=0.12.21
ARG UV_SHA256_AMD64=23f02075b652bb1df64178cfae41b5caf160822e720e2663568f3f5d63bc52c0
ARG UV_SHA256_ARM64=030b69227b40af8c1981b7301793dc66e71ed3c796ea8688209dd268bd91ec51

# SkillSpector and SkillEvaluator, the two NVIDIA skill-checking tools
# open-software-factory/software-factory#196 needs on PATH for
# `osf lint skill`. Pinned to a commit, not a tag, per that issue.
ARG SKILLSPECTOR_VERSION=2.12.0
ARG SKILLSPECTOR_COMMIT=2226747
ARG SKILLEVALUATOR_VERSION=0.4.0
ARG SKILLEVALUATOR_COMMIT=0da4646

# The coding agents this project supports, every one of them usable as a
# builder or a reviewer (see the [agents] table of osf.toml, selected from
# crates/osf/src/agents.rs, for the one list of them, their hook wiring,
# and the default builder). Each is pinned to one exact npm version so a
# rebuild in a month installs the same tool a reviewer just tested against.
ARG CODEX_VERSION=0.154.0
ARG CLAUDE_CODE_VERSION=2.1.284
# opencode's upstream npm package is opencode-ai; @opencode/cli is a
# different, much newer package under a different GitHub org. This
# project's own opencode plugin (integrations/opencode) already depends on
# @opencode-ai/plugin, opencode-ai's own typed plugin SDK, which only ever
# matches opencode-ai's own version line. Pinned to the exact version that
# plugin SDK version also exists at.
ARG OPENCODE_VERSION=1.18.31
ARG OMP_VERSION=18.2.8

# The skills tool version and the full archify commit, each pinned exactly.
ARG SKILLS_CLI_VERSION=1.7.0
ARG ARCHIFY_COMMIT=d5a1333d7447c866a765adac7d4d062f2f02e4d2

# omp's own bin/cli.js has a "#!/usr/bin/env bun" shebang: omp runs on Bun,
# not plain Node, unlike every other coding agent in this image.
ARG BUN_VERSION=1.4.2
ARG BUN_SHA256_AMD64=36368faef7527875d5ffa52e53cd48021741f2a83eb6208a8dd64068d422a913
ARG BUN_SHA256_ARM64=54328bbc2d9c8e0c9f892c544d66c57a83b84139e34909e5ee81758f1ac8fda7

RUN apt-get update && apt-get install -y --no-install-recommends \
      git \
      ca-certificates \
      curl \
      jq \
      xz-utils \
      unzip \
      bubblewrap \
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

# GitHub CLI, from its own release tarball, checked against its published
# SHA-256.
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) GH_ARCH=amd64; GH_SHA256="${GH_SHA256_AMD64}" ;; \
      arm64) GH_ARCH=arm64; GH_SHA256="${GH_SHA256_ARM64}" ;; \
      *) echo "gh: unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/gh.tar.gz \
      "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_${GH_ARCH}.tar.gz"; \
    echo "${GH_SHA256}  /tmp/gh.tar.gz" | sha256sum -c -; \
    tar -xzf /tmp/gh.tar.gz -C /tmp "gh_${GH_VERSION}_linux_${GH_ARCH}/bin/gh"; \
    install -o root -g root -m 0755 "/tmp/gh_${GH_VERSION}_linux_${GH_ARCH}/bin/gh" /usr/local/bin/gh; \
    rm -rf /tmp/gh.tar.gz "/tmp/gh_${GH_VERSION}_linux_${GH_ARCH}"; \
    gh --version

# actionlint, from its own release tarball, checked against its published
# SHA-256.
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) ACTIONLINT_ARCH=amd64; ACTIONLINT_SHA256="${ACTIONLINT_SHA256_AMD64}" ;; \
      arm64) ACTIONLINT_ARCH=arm64; ACTIONLINT_SHA256="${ACTIONLINT_SHA256_ARM64}" ;; \
      *) echo "actionlint: unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/actionlint.tar.gz \
      "https://github.com/rhysd/actionlint/releases/download/v${ACTIONLINT_VERSION}/actionlint_${ACTIONLINT_VERSION}_linux_${ACTIONLINT_ARCH}.tar.gz"; \
    echo "${ACTIONLINT_SHA256}  /tmp/actionlint.tar.gz" | sha256sum -c -; \
    tar -xzf /tmp/actionlint.tar.gz -C /tmp actionlint; \
    install -o root -g root -m 0755 /tmp/actionlint /usr/local/bin/actionlint; \
    rm -f /tmp/actionlint.tar.gz /tmp/actionlint; \
    actionlint -version

# uv, from its own release tarball, checked against its published SHA-256.
# Used only to install SkillSpector and SkillEvaluator, below.
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) UV_TARGET=x86_64-unknown-linux-gnu; UV_SHA256="${UV_SHA256_AMD64}" ;; \
      arm64) UV_TARGET=aarch64-unknown-linux-gnu; UV_SHA256="${UV_SHA256_ARM64}" ;; \
      *) echo "uv: unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/uv.tar.gz \
      "https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-${UV_TARGET}.tar.gz"; \
    echo "${UV_SHA256}  /tmp/uv.tar.gz" | sha256sum -c -; \
    tar -xzf /tmp/uv.tar.gz -C /tmp "uv-${UV_TARGET}/uv" "uv-${UV_TARGET}/uvx"; \
    install -o root -g root -m 0755 "/tmp/uv-${UV_TARGET}/uv" /usr/local/bin/uv; \
    install -o root -g root -m 0755 "/tmp/uv-${UV_TARGET}/uvx" /usr/local/bin/uvx; \
    rm -rf /tmp/uv.tar.gz "/tmp/uv-${UV_TARGET}"; \
    uv --version

# SkillSpector and SkillEvaluator, each installed from its own git history
# at the pinned commit above, per open-software-factory/software-factory#196.
# UV_TOOL_BIN_DIR puts both binaries on the same root-owned PATH as every
# other tool in this image, rather than under root's own home directory.
# UV_PYTHON_INSTALL_DIR matters just as much: left at its default, uv
# downloads the Python interpreter each tool's venv symlinks to under a
# path inside root's own home directory, which stays unreadable to dev
# even after chmod on /opt/factory/uv-tools itself. Moving it beside the
# tools keeps every path dev needs to read and execute out from under
# root's own home.
ENV UV_TOOL_DIR=/opt/factory/uv-tools
ENV UV_TOOL_BIN_DIR=/usr/local/bin
ENV UV_PYTHON_INSTALL_DIR=/opt/factory/uv-python
RUN uv tool install "git+https://github.com/NVIDIA/skillspector.git@${SKILLSPECTOR_COMMIT}" && \
    uv tool install "git+https://github.com/NVIDIA/SkillEvaluator.git@${SKILLEVALUATOR_COMMIT}" && \
    chown -R root:root /opt/factory/uv-tools /opt/factory/uv-python && \
    chmod -R a+rX /opt/factory/uv-tools /opt/factory/uv-python && \
    skillspector --help >/dev/null && \
    skillevaluator --help >/dev/null && \
    chmod -R go-w /opt/factory/uv-tools /opt/factory/uv-python

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

# Bun, from its own release zip, checked against its published SHA-256.
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) BUN_TARGET=linux-x64; BUN_SHA256="${BUN_SHA256_AMD64}" ;; \
      arm64) BUN_TARGET=linux-aarch64; BUN_SHA256="${BUN_SHA256_ARM64}" ;; \
      *) echo "bun: unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/bun.zip \
      "https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-${BUN_TARGET}.zip"; \
    echo "${BUN_SHA256}  /tmp/bun.zip" | sha256sum -c -; \
    unzip -q /tmp/bun.zip -d /tmp; \
    install -o root -g root -m 0755 "/tmp/bun-${BUN_TARGET}/bun" /usr/local/bin/bun; \
    ln -s /usr/local/bin/bun /usr/local/bin/bunx; \
    rm -rf /tmp/bun.zip "/tmp/bun-${BUN_TARGET}"; \
    bun --version

# The coding agents, installed as global npm packages into this same Node
# install. That install is already root-owned and already on PATH, so the
# install needs no separate prefix. npm still writes group- and other-
# writable files for some packages, so the write bits are stripped after
# the install. Every one of them is usable as a builder or a reviewer; see
# the [agents] table of osf.toml and crates/osf/src/agents.rs.
RUN npm install -g \
      @openai/codex@${CODEX_VERSION} \
      @anthropic-ai/claude-code@${CLAUDE_CODE_VERSION} \
      opencode-ai@${OPENCODE_VERSION} \
      @oh-my-pi/pi-coding-agent@${OMP_VERSION} && \
    codex --version && \
    claude --version && \
    opencode --version && \
    omp --version && \
    chown -R root:root /usr/local/lib/nodejs && \
    chmod -R go-w /usr/local/lib/nodejs

# dsh from its own lockfile. To update: edit .devcontainer/dsh/package.json, regenerate the lockfile, rebuild.
COPY .devcontainer/dsh/package.json .devcontainer/dsh/package-lock.json /opt/dsh/
RUN cd /opt/dsh && \
    npm ci --no-audit --no-fund && \
    chown -R root:root /opt/dsh && \
    chmod -R go-w /opt/dsh && \
    ln -s /opt/dsh/node_modules/.bin/dsh /usr/local/bin/dsh && \
    dsh --version

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

# The dsh, omp, and OpenCode writing-check integrations, built from this
# same workspace's pnpm lockfile so their dependency versions match exactly
# what `pnpm -r test` already proves elsewhere in CI. The build directory
# sits outside /workspace for the same reason the osf build above does.
WORKDIR /usr/src/factory-build
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps ./apps
COPY packages ./packages
COPY integrations ./integrations
RUN pnpm install --frozen-lockfile && \
    pnpm --filter ./integrations/dsh --filter ./integrations/omp --filter ./integrations/opencode run build

# moon runs every factory check; pinned by version and checksum, matched to
# the build's processor architecture the same way git-town is above. The
# release ships a tar.xz holding the binary alongside its docs, not a
# bare binary, so this extracts the one file it needs.
ARG MOON_VERSION=2.5.5
ARG MOON_SHA256_AMD64=f397057a22c88ff9e4b28a4a2fbf01d99f4d9a4f35e5a42634255a9f509ccbd6
ARG MOON_SHA256_ARM64=39d55e88774677181d1af3566c4ccd2994cd2d9d8199fcf30dc2d6defc05ac78
RUN set -eu; \
    case "${TARGETARCH}" in \
      amd64) MOON_ARCH=x86_64-unknown-linux-gnu; MOON_SHA256=${MOON_SHA256_AMD64} ;; \
      arm64) MOON_ARCH=aarch64-unknown-linux-gnu; MOON_SHA256=${MOON_SHA256_ARM64} ;; \
      *) echo "moon: unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/moon.tar.xz \
      "https://github.com/moonrepo/moon/releases/download/v${MOON_VERSION}/moon_cli-${MOON_ARCH}.tar.xz"; \
    echo "${MOON_SHA256}  /tmp/moon.tar.xz" | sha256sum -c -; \
    tar -xJf /tmp/moon.tar.xz -C /tmp "moon_cli-${MOON_ARCH}/moon"; \
    install -o root -g root -m 0555 "/tmp/moon_cli-${MOON_ARCH}/moon" /opt/factory/bin/moon; \
    rm -rf /tmp/moon.tar.xz "/tmp/moon_cli-${MOON_ARCH}"; \
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

# The git wrapper forces this hooks path itself, on the command line, for
# every git call under the workspace (see the wrapper's own comments and
# docs/development.md); no system-wide core.hooksPath is set here. A
# system-wide setting would apply to every repository with no local
# override of its own, including a test's own scratch repository under
# /tmp, which is exactly what this wrapper is built to avoid.
#
# The git wrapper: refuses --no-verify and -n on commit, and --no-verify
# on push, then calls the real git. Root-owned and read-only for the same
# reason as osf above. docs/development.md states plainly what it cannot do.
COPY .devcontainer/git-wrapper /opt/factory/bin/git
RUN chown root:root /opt/factory/bin/git && chmod 0555 /opt/factory/bin/git

# Every coding agent's own hook settings, root-owned and read-only, each
# calling osf hook for the one or two events osf hook supports (stop and
# prompt; osf hook has no pre-tool-use event, so none is wired to one). The
# hook files come from `osf hooks install --agents` and the [agents] table
# of osf.toml, selected from crates/osf/src/agents.rs.

# Codex: its requirements file plus its managed hooks directory, both
# confirmed, from the installed codex binary's own strings, as real
# system-level paths ("/etc/codex/config.toml", "/etc/codex/requirements.toml").
# Codex's HookEventName enum has no turn-end event, so only the prompt
# event is wired for it.
RUN mkdir -p /etc/codex/hooks
COPY .devcontainer/hooks/codex/config.toml /etc/codex/config.toml
COPY .devcontainer/hooks/codex/requirements.toml /etc/codex/requirements.toml
COPY .devcontainer/hooks/codex/hooks/osf-prompt.toml /etc/codex/hooks/osf-prompt.toml
RUN chown -R root:root /etc/codex && \
    chmod 0555 /etc/codex /etc/codex/hooks && \
    chmod 0444 /etc/codex/config.toml /etc/codex/requirements.toml /etc/codex/hooks/osf-prompt.toml

ENV PATH="/opt/factory/bin:${PATH}"

# Read by the git wrapper (.devcontainer/git-wrapper) to decide whether a
# repository sits under the workspace mount; overridable at "docker run"
# for a container started with a different bind mount.
ENV OSF_WORKSPACE_ROOT="${WORKSPACE_ROOT}"

# The one setting that names the default builder agent; see the [agents]
# table of osf.toml, selected from crates/osf/src/agents.rs, for the full
# list, and docs/development.md for how to use it. No script in this image
# chooses a builder by name: every one reads this instead.
ENV OSF_DEFAULT_BUILDER=dsh

# Every login shell resets PATH before reading /etc/profile.d/*.sh (see
# .devcontainer/profile.d/osf-path.sh), which otherwise drops osf, the
# cargo toolchain, and every coding agent's own binary.
COPY .devcontainer/profile.d/osf-path.sh /etc/profile.d/osf-path.sh
RUN chown root:root /etc/profile.d/osf-path.sh && chmod 0444 /etc/profile.d/osf-path.sh

# Proves the git wrapper's refusals and the forced hooks path at build
# time, so a broken wrapper fails the build instead of shipping quietly.
# See .devcontainer/tests/git-wrapper.sh.
COPY .devcontainer/tests/git-wrapper.sh /tmp/git-wrapper-test.sh
RUN sh /tmp/git-wrapper-test.sh && rm -f /tmp/git-wrapper-test.sh

# Non-root container user, with no password-less root access: no entry
# for it in /etc/sudoers, and it is not in the sudo group.
RUN useradd --create-home --shell /bin/bash dev

# The toolchain under /usr/local/cargo and /usr/local/rustup stays
# root-owned, so dev cannot replace rustc or cargo. dev gets its own
# writable CARGO_HOME for its registry cache and config; the cargo/rustc
# binaries are still found via /usr/local/cargo/bin, already on PATH.
ENV CARGO_HOME=/home/dev/.cargo

# rustfmt and clippy, added to the image's own pinned toolchain (the base
# image's default, not a newer one) before the toolchain is made
# read-only below, since dev cannot run rustup afterwards.
RUN rustup component add rustfmt clippy && \
    rustfmt --version && \
    cargo-clippy --version

# The base image ships this toolchain group- and other-writable, on both
# directories and many of the binaries and libraries themselves, so dev
# could still replace or edit rustc or cargo in place. Strip only the
# write bits; nothing here needs to be executable-only or unreadable.
RUN find /usr/local/cargo /usr/local/rustup -perm /022 -exec chmod go-w {} +

# dsh's "factory" and "headless" profiles: the writing-check plugin
# registered in both, from integrations/dsh's own profile-patch.example.yml.
# dsh has no system-wide config path (see the [agents] table of osf.toml and
# crates/osf/src/agents.rs). The same chown-then-chown-back order as the
# install block below applies here: mkdir -p creates .dsh itself as root, so
# dev needs it back before only the two profile directories are locked
# root-owned and read-only. dsh creates the "headless" profile itself on its
# first run with an empty cordis.patch.yml, so installing it here gives that
# profile the plugin too. Run `dsh --profile factory` or
# `dsh --profile headless` to boot one. Each profile also registers the
# Claude-format hooks bridge, pointed at this repository's own
# .claude/hooks.json (see the profile's own comments for why that file is not
# under this root-owned directory).
#
# dsh 0.2 needs each profile's own package.json and the plugin under its
# node_modules. It also rewrites an empty cordis.yml in the profile on every
# start, so only that file is dev-owned.
COPY .devcontainer/hooks/dsh/profile.package.json .devcontainer/hooks/dsh/cordis.patch.yml /opt/dsh-profile/
RUN set -eu; \
    for profile in factory headless; do \
      P="${DEV_HOME}/.dsh/profiles/${profile}"; \
      mkdir -p "${P}/node_modules/@open-software-factory/osf-dsh-plugin"; \
      cp /opt/dsh-profile/profile.package.json "${P}/package.json"; \
      cp /opt/dsh-profile/cordis.patch.yml "${P}/cordis.patch.yml"; \
      cp /usr/src/factory-build/integrations/dsh/package.json \
        /usr/src/factory-build/integrations/dsh/cordis.patch.yml \
        "${P}/node_modules/@open-software-factory/osf-dsh-plugin/"; \
      mkdir "${P}/node_modules/@open-software-factory/osf-dsh-plugin/dist"; \
      cp -r /usr/src/factory-build/integrations/dsh/dist/src \
        "${P}/node_modules/@open-software-factory/osf-dsh-plugin/dist/src"; \
    done; \
    chown -R dev:dev "${DEV_HOME}/.dsh"; \
    for profile in factory headless; do \
      P="${DEV_HOME}/.dsh/profiles/${profile}"; \
      chown -R root:root "${P}"; \
      find "${P}" -type d -exec chmod 0555 {} +; \
      find "${P}" -type f -exec chmod 0444 {} +; \
      install -o dev -g dev -m 0644 /dev/null "${P}/cordis.yml"; \
    done; \
    rm -rf /opt/dsh-profile

# The home hook files from osf.toml, written by `osf hooks install --agents`; the home copies stay.
# The system copies below are the ones a changed HOME cannot move.
# omp and dsh have no system-wide location, so their hook files stay under the home folder only.
COPY osf.toml /tmp/osf-hooks/osf.toml
RUN set -eu; \
    stamp=/tmp/osf-hooks.stamp; \
    touch "$stamp"; \
    output="$(cd /tmp/osf-hooks && osf hooks install --agents --root "${DEV_HOME}")"; \
    printf '%s\n' "$output"; \
    find "${DEV_HOME}" -type d -newer "$stamp" -exec chown dev:dev {} +; \
    printf '%s\n' "$output" | while IFS= read -r line; do \
      case "$line" in \
        "osf hooks install: wrote "*) \
          file="${line#osf hooks install: wrote }"; \
          # Both dsh profiles carry the plugin, so this global file must not be left behind.
          case "$file" in \
            */.dsh/cordis.patch.yml) rm -f "$file"; continue ;; \
          esac; \
          chown root:root "$file"; \
          chmod 0444 "$file"; \
          dir="$(dirname "$file")"; \
          chown root:root "$dir"; \
          chmod 1777 "$dir" ;; \
      esac; \
    done; \
    for pair in "${DEV_HOME}/.claude/settings.json:/etc/claude-code/managed-settings.json" \
                "${DEV_HOME}/.config/opencode/opencode.json:/etc/opencode/opencode.json"; do \
      src="${pair%%:*}"; \
      dst="${pair#*:}"; \
      if [ -f "$src" ]; then \
        mkdir -p "$(dirname "$dst")"; \
        install -o root -g root -m 0444 "$src" "$dst"; \
        chown root:root "$(dirname "$dst")"; \
        chmod 0555 "$(dirname "$dst")"; \
      fi; \
    done; \
    rm -rf /tmp/osf-hooks "$stamp"

# The omp and opencode hook files name their npm packages; install the
# built packages under the home's node_modules, where node resolves them.
RUN set -eu; \
    for pair in omp:osf-omp-hook opencode:osf-opencode-plugin; do \
      src="${pair%%:*}"; \
      name="${pair#*:}"; \
      target="${DEV_HOME}/node_modules/@open-software-factory/${name}"; \
      mkdir -p "$target"; \
      cp "/usr/src/factory-build/integrations/${src}/package.json" "$target/package.json"; \
      cp -r "/usr/src/factory-build/integrations/${src}/dist" "$target/dist"; \
    done; \
    chown -R root:root "${DEV_HOME}/node_modules"; \
    find "${DEV_HOME}/node_modules" -type d -exec chmod 0555 {} +; \
    find "${DEV_HOME}/node_modules" -type f -exec chmod 0444 {} +

# The node_modules build tree for the three plugins above is no longer
# needed once their dist/ output is copied out.
RUN rm -rf /usr/src/factory-build

WORKDIR ${WORKSPACE_ROOT}

USER dev

# Third-party skills, never vendored; list home: open-software-factory/software-factory#224.
# The tree URL pins the commit and names each skill by path, so discovery cannot drop one.
# The skills tool does not know dsh or omp, so only opencode, codex and claude-code are named.
RUN npx -y "skills@${SKILLS_CLI_VERSION}" add \
      "https://github.com/tt-a1i/archify/tree/${ARCHIFY_COMMIT}/archify" \
      -g --agent opencode codex claude-code --copy -y && \
    npx -y "skills@${SKILLS_CLI_VERSION}" add \
      "https://github.com/tt-a1i/archify/tree/${ARCHIFY_COMMIT}/.agents/skills/archify-review" \
      -g --agent opencode codex claude-code --copy -y

# Proves the skills above are installed and not empty. See
# .devcontainer/tests/skills-installed.sh.
COPY --chown=dev:dev .devcontainer/tests/skills-installed.sh /tmp/skills-installed-test.sh
RUN sh /tmp/skills-installed-test.sh && rm -f /tmp/skills-installed-test.sh

# Proves dsh starts, with no credentials, on both profiles the image uses:
# each must stop at the missing credential and nothing else. See
# .devcontainer/tests/dsh-starts.sh.
COPY --chown=dev:dev .devcontainer/tests/dsh-starts.sh /tmp/dsh-starts-test.sh
RUN sh /tmp/dsh-starts-test.sh && rm -f /tmp/dsh-starts-test.sh

# Proves each coding agent runs for the dev user, not just for root.
# See .devcontainer/tests/coding-agent-tools.sh.
COPY --chown=dev:dev .devcontainer/tests/coding-agent-tools.sh /tmp/coding-agent-tools-test.sh
RUN sh /tmp/coding-agent-tools-test.sh && rm -f /tmp/coding-agent-tools-test.sh

# Proves every builder tool runs for the dev user, including from a login
# shell. See .devcontainer/tests/builder-tools.sh.
COPY --chown=dev:dev .devcontainer/tests/builder-tools.sh /tmp/builder-tools-test.sh
RUN sh /tmp/builder-tools-test.sh && rm -f /tmp/builder-tools-test.sh

# Proves the hook wiring itself: every enabled agent's hook file is
# root-owned and read-only, and osf hook answers a sample payload for each
# event it supports. See .devcontainer/tests/agent-hooks.sh. The script
# reads the [agents] table from the osf.toml copied beside it, and runs
# from that folder.
COPY --chown=dev:dev osf.toml /tmp/agent-hooks-test/osf.toml
COPY --chown=dev:dev .devcontainer/tests/agent-hooks.sh /tmp/agent-hooks-test/tests/agent-hooks.sh
RUN cd /tmp/agent-hooks-test && sh tests/agent-hooks.sh && rm -rf /tmp/agent-hooks-test

# Proves the git hooks: executable, every osf flag they use exists in the
# installed osf, and each runs in a scratch repository. The copy of the
# source hooks keeps the mode git stored, which the test also checks. See
# .devcontainer/tests/git-hooks.sh.
COPY --chown=dev:dev .devcontainer/githooks /tmp/git-hooks-test/githooks
COPY --chown=dev:dev .devcontainer/tests/git-hooks.sh /tmp/git-hooks-test/git-hooks.sh
RUN sh /tmp/git-hooks-test/git-hooks.sh /tmp/git-hooks-test/githooks && rm -rf /tmp/git-hooks-test

# Proves every root-owned toolchain folder is read-only for the dev user.
# See .devcontainer/tests/toolchain-readonly.sh.
COPY --chown=dev:dev .devcontainer/tests/toolchain-readonly.sh /tmp/toolchain-readonly-test.sh
RUN sh /tmp/toolchain-readonly-test.sh && rm -f /tmp/toolchain-readonly-test.sh
