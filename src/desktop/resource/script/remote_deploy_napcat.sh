#!/usr/bin/env bash
set -euo pipefail

workspace_dir="${workspace_dir:-$HOME/NapCatCore}"
runtime_dir="${runtime_dir:-$workspace_dir/runtime}"
config_dir="${config_dir:-$runtime_dir/config}"
log_dir="${log_dir:-$runtime_dir/log}"
tmp_dir="${tmp_dir:-$runtime_dir/tmp}"
package_dir="${package_dir:-$workspace_dir/packages}"
config_archive="${config_archive:-$tmp_dir/config-export.zip}"
status_file="${status_file:-$runtime_dir/status.json}"

mkdir -p "$workspace_dir" "$runtime_dir" "$config_dir" "$log_dir" "$tmp_dir" "$package_dir"

if [ ! -f "$config_archive" ]; then
  echo "[ERROR] config archive not found: $config_archive" >&2
  exit 20
fi

if command -v unzip >/dev/null 2>&1; then
  unzip -o "$config_archive" -d "$config_dir" >/dev/null
elif command -v python3 >/dev/null 2>&1; then
  python3 - "$config_archive" "$config_dir" <<'PY'
import sys
import pathlib
import zipfile

archive = pathlib.Path(sys.argv[1])
target = pathlib.Path(sys.argv[2])
target.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(archive, 'r') as zip_file:
    zip_file.extractall(target)
PY
else
  echo "[ERROR] unzip and python3 are both unavailable" >&2
  exit 21
fi

cat > "$status_file" <<EOF
{
  "running": false,
  "pid": null,
  "qq": null,
  "version": null,
  "log_file": "$log_dir/napcat.log",
  "last_action": "deploy",
  "last_error": null,
  "updated_at": "$(date -Iseconds)"
}
EOF

echo "[OK] remote deploy finished"
