#!/usr/bin/env bash
set -euo pipefail

npm install -g @scope/tool
npm install -g @scope/pinned@1.2.3
npm install -g plain-tool
npm install -g plain-pinned@2.0.0
pip install helper
pip install helper==3.1.0
docker pull example/image:latest
