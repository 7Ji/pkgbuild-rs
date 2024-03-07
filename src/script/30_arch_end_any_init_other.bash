  echo END
  _arch_collapsed="${arch[*]}"
  if [[ " ${_arch_collapsed} " =~ ' any ' ]]; then
    if [[ "${#_arch_collapsed}" != 5 ]]; then
      echo "ERROR: Architecture 'any' found when multiple architecture defined"
      exit 2
    fi
  else
    for _arch in "${arch[@]}"; do
      echo ARCH
      echo arch:"${_arch}"
