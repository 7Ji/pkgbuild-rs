  echo END
  _arch_collapsed="${arch[*]}"
  if [[ " ${_arch_collapsed} " =~ (^| )any( |$) ]]; then
    if [[ "${#_arch_collapsed}" != 3 ]]; then
      echo "ERROR: PKGBUILD Architecture 'any' found when multiple architecture defined"
      exit 2
    fi
  else
    for _arch in "${arch[@]}"; do
      echo ARCH
      echo arch:"${_arch}"
