#!/usr/bin/env bash
# Assembles submission video: title card + takes + end card.
#
# Usage:
#   ./zloz.sh surowe/ujecie-2-*.mp4 surowe/ujecie-3-*.mp4 ...
set -euo pipefail

KAT="$(cd "$(dirname "$0")" && pwd)"
OKLADKA="$KAT/../okladka-1920x1080.png"
ROBOCZY="$(mktemp -d)"
trap 'rm -rf "$ROBOCZY"' EXIT

[ -f "$OKLADKA" ] || { echo "missing cover: $OKLADKA" >&2; exit 1; }
[ $# -ge 1 ] || { echo "provide recorded takes in order" >&2; exit 1; }

# Cards: 4s video with fade in and fade out.
plansza() {
    ffmpeg -hide_banner -loglevel error -y -loop 1 -t 4 -i "$OKLADKA" \
        -vf "scale=1920:1080,fade=in:0:20,fade=out:70:20,format=yuv420p" \
        -r 30 -c:v libx264 -preset medium -crf 18 "$1"
}
plansza "$ROBOCZY/tytul.mp4"
plansza "$ROBOCZY/koniec.mp4"

# Normalize takes to matching parameters.
LISTA="$ROBOCZY/lista.txt"
echo "file '$ROBOCZY/tytul.mp4'" > "$LISTA"
i=0
for u in "$@"; do
    [ -f "$u" ] || { echo "missing file: $u" >&2; exit 1; }
    i=$((i + 1))
    N="$ROBOCZY/u$i.mp4"
    # Silent takes get silence audio track so concat preserves audio.
    ffmpeg -hide_banner -loglevel error -y -i "$u" \
        -f lavfi -i anullsrc=channel_layout=stereo:sample_rate=48000 \
        -shortest -map 0:v:0 -map "1:a:0?" \
        -vf "scale=1920:1080,format=yuv420p" -r 30 \
        -c:v libx264 -preset medium -crf 18 -c:a aac -b:a 160k "$N" 2>/dev/null \
    || ffmpeg -hide_banner -loglevel error -y -i "$u" \
        -vf "scale=1920:1080,format=yuv420p" -r 30 \
        -c:v libx264 -preset medium -crf 18 -an "$N"
    echo "file '$N'" >> "$LISTA"
done
echo "file '$ROBOCZY/koniec.mp4'" >> "$LISTA"

WYJSCIE="$KAT/SIMON-wideo.mp4"
ffmpeg -hide_banner -loglevel warning -y -f concat -safe 0 -i "$LISTA" \
    -c:v libx264 -preset slow -crf 19 -pix_fmt yuv420p -r 30 \
    -movflags +faststart "$WYJSCIE"

echo
ffprobe -hide_banner -v error -show_entries format=duration,size \
    -show_entries stream=width,height,codec_name,avg_frame_rate \
    -of default=noprint_wrappers=1 "$WYJSCIE"
echo "-> $WYJSCIE"
echo
echo "Check duration: recommended target is 2-5 minutes."
