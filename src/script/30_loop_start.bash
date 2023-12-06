while read -r line; do
  source "${line}"
  echo "[PKGBUILD]"
  pkgbase="${pkgbase:-${pkgname}}"
