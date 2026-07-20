#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 /path/to/moine.whl" >&2
  exit 2
fi

wheel="$1"
test -f "$wheel"
python_bin="${MOINE_SMOKE_PYTHON:-python}"

"$python_bin" -m pip install --disable-pip-version-check --force-reinstall \
  "$wheel" 'pytest>=8,<9'
"$python_bin" -I -c 'from importlib.metadata import version; import moine; assert moine.__version__ == version("moine"); assert moine.distance("abc", "adc") == 1; assert moine.damerau_distance("abc", "acb") == 1; print(moine.__version__, moine.__file__)'
"$python_bin" -m pip check
"$python_bin" -m pytest python/tests
