#!/usr/bin/env bash
# Nagrywanie ekranu pod wideo zgłoszeniowe.
#
# Nagrywamy DOKŁADNIE 1920×1080 z ekranu, nie cały monitor 4K przeskalowany —
# przeskalowany tekst w terminalu robi się nieczytelny, a w tym wideo liczy się
# właśnie to, że widać treść receiptu.
#
# Kodowanie na CPU (libx264), NIE na NVENC: karta w trakcie nagrania liczy
# zlecenia dla demo. Nagrywarka nie może konkurować z tym, co pokazuje.
#
# Użycie:
#   ./nagrywaj.sh ujecie-2                 # nagrywa do przerwania (q albo Ctrl+C)
#   ./nagrywaj.sh ujecie-2 --z-dzwiekiem   # dokłada narrację z domyślnego mikrofonu
#   OFFSET=1080,0 ./nagrywaj.sh ujecie-2   # inny róg ekranu
set -euo pipefail

NAZWA="${1:-ujecie}"
KAT="$(cd "$(dirname "$0")" && pwd)/surowe"
mkdir -p "$KAT"
WYJSCIE="$KAT/${NAZWA}-$(date +%H%M%S).mp4"

SZEROKOSC=1920
WYSOKOSC=1080
# Domyślnie lewy górny róg monitora głównego. `xrandr` zna jego przesunięcie —
# wpisywanie tego z palca to najczęstsza przyczyna nagrania w złym miejscu.
if [ -z "${OFFSET:-}" ]; then
    read -r OX OY < <(xrandr --current 2>/dev/null \
        | awk '/ connected primary/ {match($0, /[0-9]+x[0-9]+\+([0-9]+)\+([0-9]+)/, m); print m[1], m[2]; exit}')
    OFFSET="${OX:-0},${OY:-0}"
fi
echo "obszar: ${SZEROKOSC}x${WYSOKOSC} od +${OFFSET/,/+}  (DISPLAY=${DISPLAY:-:0})"

ARG_AUDIO=()
if [ "${2:-}" = "--z-dzwiekiem" ]; then
    ZRODLO="$(pactl get-default-source 2>/dev/null || echo default)"
    echo "dźwięk: $ZRODLO"
    echo "UWAGA: sprawdź, czy to na pewno mikrofon, którego chcesz użyć —"
    echo "       domyślnym bywa kamerka, nie zestaw słuchawkowy."
    ARG_AUDIO=(-f pulse -ac 2 -i "$ZRODLO" -c:a aac -b:a 160k)
fi

for i in 3 2 1; do printf "\rstart za %s..." "$i"; sleep 1; done; echo -e "\rnagrywam — 'q' kończy   "

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
