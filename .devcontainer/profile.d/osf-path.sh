# Read by every login shell: /etc/profile resets PATH to its own default
# before sourcing /etc/profile.d/*.sh, which drops osf, the cargo
# toolchain, and the Node install every coding agent's binary lives
# under. This file puts all three back, so `bash -lc '...'`, used by this
# project's own test scripts and by a harness that boots a login shell,
# finds them. See .devcontainer/tests/builder-tools.sh.
export PATH="/opt/factory/bin:/usr/local/cargo/bin:/usr/local/lib/nodejs/node/bin:${PATH}"
