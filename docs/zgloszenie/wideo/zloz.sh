#!/usr/bin/env bash
# Składa wideo zgłoszeniowe: plansza tytułowa + ujęcia + plansza końcowa.
#
# Plansze robimy z okładki, którą już mamy — jedna tożsamość wizualna dla
# całego zgłoszenia, zero generowania czegokolwiek.
#
# Użycie:
#   ./zloz.sh surowe/ujecie-2-*.mp4 surowe/ujecie-3-*.mp4 ...
set -euo pipefail

KAT="$(cd "$(dirname "$0")" && pwd)"
OKLADKA="$KAT/../okladka-1920x1080.png"
ROBOCZY="$(mktemp -d)"
trap 'rm -rf "$ROBOCZY"' EXIT

[ -f "$OKLADKA" ] || { echo "brak okładki: $OKLADKA" >&2; exit 1; }
[ $# -ge 1 ] || { echo "podaj nagrane ujęcia w kolejności" >&2; exit 1; }

# Plansze: 4 s obrazu z wejściem i wyjściem przez fade.
plansza() {
    ffmpeg -hide_banner -loglevel error -y -loop 1 -t 4 -i "$OKLADKA" \
        -vf "scale=1920:1080,fade=in:0:20,fade=out:70:20,format=yuv420p" \
        -r 30 -c:v libx264 -preset medium -crf 18 "$1"
}
plansza "$ROBOCZY/tytul.mp4"
plansza "$ROBOCZY/koniec.mp4"

# Ujęcia normalizujemy do wspólnych parametrów — inaczej konkatenacja
# strumieni o różnym fps albo formacie pikseli daje zacinający się plik.
LISTA="$ROBOCZY/lista.txt"
echo "file '$ROBOCZY/tytul.mp4'" > "$LISTA"
i=0
for u in "$@"; do
    [ -f "$u" ] || { echo "brak pliku: $u" >&2; exit 1; }
    i=$((i + 1))
    N="$ROBOCZY/u$i.mp4"
    # Ciche ujęcia dostają ścieżkę ciszy, żeby konkatenacja nie zgubiła audio
    # tam, gdzie któreś ujęcie MA narrację.
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
echo "Sprawdź długość: lablab lubi 2-5 minut. Dłuższe wideo nikt nie dogląda do końca."
