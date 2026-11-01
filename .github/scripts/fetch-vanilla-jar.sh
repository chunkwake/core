#!/usr/bin/env bash
# Скачивает `server.jar` нужной версии по манифесту Mojang — с повторами.
#
# Запуск: fetch-vanilla-jar.sh <версия> [каталог]
#
# `server.jar` уже на месте — ничего не делает: в CI его приносит кэш, и
# повторный запуск не должен качать 60 МБ.
#
# Повторы нужны из-за 60 МБ: одиночный `curl` без `--retry-all-errors` на
# обрыве соединения не повторяет вовсе (`-f` считает сетевую ошибку
# неисправимой). Именно так падал Windows-раннер на `Invoke-WebRequest`.
set -euo pipefail

version="${1:?версия не задана}"
workdir="${2:-.}"
mkdir -p "$workdir"
cd "$workdir"

if [ -f server.jar ]; then
  echo "server.jar уже в кэше"
  exit 0
fi

retry=(--retry 5 --retry-delay 5 --retry-all-errors --connect-timeout 20 --max-time 600)
get() { curl -fsSL "${retry[@]}" "$@"; }

# `jq` есть на ubuntu-latest; `python3`/`python` — запасной вариант. Наличие
# проверяется запуском: в Windows `python3` бывает заглушкой Microsoft Store.
py=()
for candidate in python3 python; do
  if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import json' >/dev/null 2>&1; then
    py=("$candidate")
    break
  fi
done
if ! command -v jq >/dev/null 2>&1 && [ ${#py[@]} -eq 0 ]; then
  echo "нужен jq или python для разбора манифеста Mojang" >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Манифесты качаются в файлы, а не в пайп: иначе падение разборщика рвёт
# `curl` по SIGPIPE и настоящая причина теряется в шуме.
get -o "$work/versions.json" https://piston-meta.mojang.com/mc/game/version_manifest_v2.json

if command -v jq >/dev/null 2>&1; then
  meta_url=$(jq -r --arg v "$version" \
    '.versions[] | select(.id == $v) | .url' "$work/versions.json")
else
  meta_url=$(CHUNKWAKE_VERSION="$version" "${py[@]}" -c \
    'import json,os,sys; d=json.load(open(sys.argv[1]));
print("\n".join(v["url"] for v in d["versions"] if v["id"] == os.environ["CHUNKWAKE_VERSION"]))' \
    "$work/versions.json")
fi
if [ -z "$meta_url" ] || [ "$meta_url" = "null" ]; then
  echo "версии $version нет в манифесте Mojang" >&2
  exit 1
fi

get -o "$work/version.json" "$meta_url"
if command -v jq >/dev/null 2>&1; then
  url=$(jq -r '.downloads.server.url' "$work/version.json")
  sha1=$(jq -r '.downloads.server.sha1' "$work/version.json")
else
  url=$("${py[@]}" -c 'import json,sys; print(json.load(open(sys.argv[1]))["downloads"]["server"]["url"])' \
    "$work/version.json")
  sha1=$("${py[@]}" -c 'import json,sys; print(json.load(open(sys.argv[1]))["downloads"]["server"]["sha1"])' \
    "$work/version.json")
fi
if [ -z "$url" ] || [ "$url" = "null" ]; then
  echo "в манифесте $version нет downloads.server" >&2
  exit 1
fi

# Недокачанный файл от прошлого запуска не должен попасть в проверку SHA-1.
rm -f server.jar
get -o server.jar "$url"
echo "$sha1  server.jar" | sha1sum -c -
