#!/usr/bin/env bash
set -euo pipefail

binary=${1:?"укажи путь к release-бинарнику"}
np_binary=${2:?"укажи путь к np release-бинарнику"}
version=${3:?"укажи версию"}
output=${4:-dist}

if [[ ! -f "$binary" ]]; then
    printf 'Бинарник не найден: %s\n' "$binary" >&2
    exit 1
fi
if [[ ! -f "$np_binary" ]]; then
    printf 'np бинарник не найден: %s\n' "$np_binary" >&2
    exit 1
fi
if ! file -b "$binary" | grep -Eq 'ELF 64-bit.*x86-64'; then
    printf 'Нужен Linux ELF x86_64: %s\n' "$binary" >&2
    exit 1
fi
if [[ ! "$version" =~ ^[0-9][0-9A-Za-z.+~-]*$ ]]; then
    printf 'Версия не подходит для deb: %s\n' "$version" >&2
    exit 1
fi

mkdir -p "$output"
output=$(cd "$output" && pwd)
archive="$output/noverplay-linux-x86_64.tar.gz"
deb="$output/noverplay_${version}_amd64.deb"
checksums="$output/SHA256SUMS"
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
stage="$work/archive"
debroot="$work/debroot"

for path in "$archive" "$deb" "$checksums"; do
    if [[ -e "$path" ]]; then
        printf 'Путь сборки уже существует: %s\n' "$path" >&2
        exit 1
    fi
done

install -d -m 0755 "$stage"
install -m 0755 "$binary" "$stage/noverplay"
install -m 0755 "$np_binary" "$stage/np"
install -m 0644 LICENSE "$stage/LICENSE"
tar -C "$stage" -czf "$archive" noverplay np LICENSE

install -d -m 0755 "$debroot/DEBIAN" "$debroot/usr/bin" "$debroot/usr/share/doc/noverplay"
install -m 0755 "$binary" "$debroot/usr/bin/noverplay"
install -m 0755 "$np_binary" "$debroot/usr/bin/np"
install -m 0644 LICENSE "$debroot/usr/share/doc/noverplay/copyright"
installed_size=$(du -ck "$binary" "$np_binary" | awk '/total/{print $1}')
printf '%s\n' \
    'Package: noverplay' \
    "Version: $version" \
    'Section: sound' \
    'Priority: optional' \
    'Architecture: amd64' \
    'Maintainer: Jselyx' \
    'Depends: libasound2 (>= 1.0.27) | libasound2t64' \
    "Installed-Size: $installed_size" \
    'Homepage: https://github.com/Jselyx/noverplay-tui' \
    'Description: терминальный музыкальный клиент Noverplay' \
    > "$debroot/DEBIAN/control"
chmod 0644 "$debroot/DEBIAN/control"

dpkg-deb --root-owner-group --build "$debroot" "$deb"
(
    cd "$output"
    sha256sum "$(basename "$archive")" "$(basename "$deb")" > "$(basename "$checksums")"
)

printf '%s\n' "$archive" "$deb" "$checksums"
