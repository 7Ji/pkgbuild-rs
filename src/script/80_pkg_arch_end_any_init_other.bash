    echo END
    if [[ " ${_arch_collapsed} " =~ (^| )any( |$) ]]; then
      if [[ "${#_arch_collapsed}" != 3 ]]; then
        echo "ERROR: Package architecture 'any' found when multiple architecture defined"
        exit 6
      fi
    else
      for _arch in "${arch[@]}"; do
        echo PACKAGEARCH
        echo arch:"${_arch}"
