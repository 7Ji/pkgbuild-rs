source_makepkg_config
_ifs_stored="${IFS}"
while read -r _line; do
(
  source "${_line}"
  echo PKGBUILD
  pkgbase="${pkgbase:-${pkgname}}"
