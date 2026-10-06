#!/usr/bin/env bash
# SIMON — AMD MI300X session orchestrator. See README.md.
# Phases run in order so one paid session covers FP arm -> node -> video -> destroy.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PHASE="${1:-all}"

case "$PHASE" in
  env)     bash "$HERE/env.sh" ;;
  build)   bash "$HERE/build_hip.sh" ;;
  fp)      bash "$HERE/fp_arm.sh" ;;
  node)    bash "$HERE/simon_node.sh" ;;
  video)   bash "$HERE/video.sh" ;;
  destroy) bash "$HERE/destroy.sh" ;;
  all)
    bash "$HERE/env.sh"
    bash "$HERE/build_hip.sh"
    bash "$HERE/fp_arm.sh"
    bash "$HERE/simon_node.sh"
    bash "$HERE/video.sh"
    echo "== done. run: bash $HERE/session.sh destroy when finished =="
    ;;
  *) echo "usage: session.sh [env|build|fp|node|video|destroy|all]" >&2; exit 2 ;;
esac
