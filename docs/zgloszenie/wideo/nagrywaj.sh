#!/usr/bin/env bash
# Screen recording for submission video.
#
# Records EXACTLY 1920x1080 from screen, not full 4K downscaled -
# scaled terminal text becomes unreadable, and this video needs crisp receipts.
#
# CPU encoding (libx264), NOT NVENC: GPU runs demo inference jobs during recording.
#
# Usage:
#   ./nagrywaj.sh ujecie-2                 # records until interrupted (q or Ctrl+C)
#   ./nagrywaj.sh ujecie-2 --z-dzwiekiem   # adds narration from default microphone
#   OFFSET=1080,0 ./nagrywaj.sh ujecie-2   # alternative screen offset
set -euo pipefail

NAZWA="${1:-ujecie}"
KAT="$(cd "$(dirname "$0")" && pwd)/surowe"
mkdir -p "$KAT"
WYJSCIE="$KAT/${NAZWA}-$(date +%H%M%S).mp4"

SZEROKOSC=1920
WYSOKOSC=1080
# Default: top-left corner of primary monitor.
if [ -z "${OFFSET:-}" ]; then
    read -r OX OY < <(xrandr --current 2>/dev/null \
        | awk '/ connected primary/ {match($0, /[0-9]+x[0-9]+\+([0-9]+)\+([0-9]+)/, m); print m[1], m[2]; exit}')
    OFFSET="${OX:-0},${OY:-0}"
fi
echo "area: ${SZEROKOSC}x${WYSOKOSC} from +${OFFSET/,/+}  (DISPLAY=${DISPLAY:-:0})"

ARG_AUDIO=()
if [ "${2:-}" = "--z-dzwiekiem" ]; then
    ZRODLO="$(pactl get-default-source 2>/dev/null || echo default)"
    echo "audio source: $ZRODLO"
    echo "NOTE: verify this is the intended microphone."
    ARG_AUDIO=(-f pulse -ac 2 -i "$ZRODLO" -c:a aac -b:a 160k)
fi

for i in 3 2 1; do printf "\rstarting in %s..." "$i"; sleep 1; done; echo -e "\rrecording - 'q' stops   "

ffmpeg -hide_banner -loglevel warning \
    -f x11grab -framerate 30 -draw_mouse 1 \
    -video_size "${SZEROKOSC}x${WYSOKOSC}" \
    -i "${DISPLAY:-:0}+${OFFSET/,/,}" \
    "${ARG_AUDIO[@]}" \
    -c:v libx264 -preset medium -crf 18 -pix_fmt yuv420p \
    -movflags +faststart \
    "$WYJSCIE"

echo
ffprobe -hide_banner -v error -show_entries format=duration -show_entries stream=width,height,codec_name \
    -of default=noprint_wrappers=1 "$WYJSCIE"
echo "-> $WYJSCIE"
