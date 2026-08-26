#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
source_binary=${1:-"$root_dir/target/release/avl-basic"}
source_icons="$root_dir/assets/linux/hicolor"
source_samples="$root_dir/samples"
desktop_template="$script_dir/linux/avl-basic.desktop.in"

if [ ! -x "$source_binary" ]; then
    echo "AVL BASIC executable not found: $source_binary" >&2
    echo "Build it first with: cargo build --release" >&2
    exit 1
fi

if [ ! -d "$source_samples" ]; then
    echo "AVL BASIC samples not found: $source_samples" >&2
    exit 1
fi

data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
bin_dir=${AVL_BASIC_BIN_DIR:-"$HOME/.local/bin"}
installed_binary="$bin_dir/avl-basic"
runtime_data_dir="$data_home/avl-basic"
installed_samples="$runtime_data_dir/samples"
desktop_file="$data_home/applications/avl-basic.desktop"

install -d "$bin_dir" "$data_home/applications" "$runtime_data_dir"
install -m 0755 "$source_binary" "$installed_binary"

samples_stage_root=$(mktemp -d "$runtime_data_dir/.samples-install.XXXXXX")
staged_samples="$samples_stage_root/new"
previous_samples="$samples_stage_root/previous"

cleanup_samples_stage() {
    if [ -n "${samples_stage_root:-}" ] && [ -d "$samples_stage_root" ]; then
        if [ -e "$previous_samples" ] || [ -L "$previous_samples" ]; then
            if [ ! -e "$installed_samples" ] && [ ! -L "$installed_samples" ]; then
                if ! mv "$previous_samples" "$installed_samples"; then
                    echo "Previous samples preserved at: $previous_samples" >&2
                    return 1
                fi
            fi
        fi
        rm -rf "$samples_stage_root"
    fi
}

trap cleanup_samples_stage 0
trap 'exit 1' HUP INT TERM

install -d -m 0755 "$staged_samples"
cp -R "$source_samples/." "$staged_samples/"

if [ -e "$installed_samples" ] || [ -L "$installed_samples" ]; then
    mv "$installed_samples" "$previous_samples"
fi

if ! mv "$staged_samples" "$installed_samples"; then
    echo "Could not replace installed AVL BASIC samples: $installed_samples" >&2
    exit 1
fi

cleanup_samples_stage
samples_stage_root=
trap - 0 HUP INT TERM

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
path_replacement=$(escape_sed_replacement "$runtime_data_dir")

sed \
    -e "s|@EXEC@|$exec_replacement|g" \
    -e "s|@PATH@|$path_replacement|g" \
    "$desktop_template" > "$desktop_file"
chmod 0644 "$desktop_file"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$data_home/applications" >/dev/null 2>&1 || true
fi

echo "Installed AVL BASIC: $installed_binary"
echo "Installed samples and gallery: $installed_samples"
echo "Installed desktop launcher: $desktop_file"
echo "For the same root in a terminal: cd \"$runtime_data_dir\" && avl-basic"
