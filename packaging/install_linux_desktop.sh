#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
source_binary=${1:-"$root_dir/target/release/avl-basic"}
source_icons="$root_dir/assets/linux/hicolor"
desktop_template="$script_dir/linux/avl-basic.desktop.in"

if [ ! -x "$source_binary" ]; then
    echo "AVL BASIC executable not found: $source_binary" >&2
    echo "Build it first with: cargo build --release" >&2
    exit 1
fi

data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
bin_dir=${AVL_BASIC_BIN_DIR:-"$HOME/.local/bin"}
installed_binary="$bin_dir/avl-basic"
desktop_file="$data_home/applications/avl-basic.desktop"

install -d "$bin_dir" "$data_home/applications"
install -m 0755 "$source_binary" "$installed_binary"

for size in 16 24 32 48 64 128 256; do
    icon_dir="$data_home/icons/hicolor/${size}x${size}/apps"
    install -d "$icon_dir"
    install -m 0644 \
        "$source_icons/${size}x${size}/apps/avl-basic.png" \
        "$icon_dir/avl-basic.png"
done

escape_exec_argument() {
    printf '%s' "$1" | sed \
        -e 's/\\/\\\\/g' \
        -e 's/"/\\"/g' \
        -e 's/`/\\`/g' \
        -e 's/\$/\\$/g'
}

escape_sed_replacement() {
    printf '%s' "$1" | sed -e 's/[\\&|]/\\&/g'
}

exec_value="\"$(escape_exec_argument "$installed_binary")\""
exec_replacement=$(escape_sed_replacement "$exec_value")

sed \
    -e "s|@EXEC@|$exec_replacement|g" \
    "$desktop_template" > "$desktop_file"
chmod 0644 "$desktop_file"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$data_home/applications" >/dev/null 2>&1 || true
fi

echo "Installed AVL BASIC: $installed_binary"
echo "Installed desktop launcher: $desktop_file"
