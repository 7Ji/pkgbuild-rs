  if [[ $(type -t pkgver) == function ]]; then
    echo pkgver_func:y
  else
    echo pkgver_func:n
  fi
  echo ARCH
  echo arch:any
