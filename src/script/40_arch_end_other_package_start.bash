      echo END
    done
  fi
  _pkg_used=''
  for _pkgname in "${pkgname[@]}"; do
  (
    echo PACKAGE
    echo pkgname:"${_pkgname}"
    if [[ $(type -t package_"${_pkgname}") == function ]]; then
      _pkg_func=package_"${_pkgname}"
    elif [[ $(type -t package) == function ]]; then
      if [[ "${_pkg_used}" ]]; then
        echo "Did not find package split function for ${_pkgname}"
        exit 3
      fi
      _pkg_func=package
      _pkg_used=y
    elif [[ "${_pkgname}" == "${pkgbase}" ]]; then
      echo END
      exit
    else
      echo "No package split function for ${_pkgname}"
      exit 4
    fi
