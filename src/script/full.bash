source_makepkg_config
_ifs_stored="${IFS}"
while read -r _line; do
(
  source "${_line}"
  echo PKGBUILD
  pkgbase="${pkgbase:-${pkgname}}"
  echo pkgbase:"${pkgbase}"
  echo pkgver:"${pkgver}"
  echo pkgrel:"${pkgrel}"
  echo epoch:"${epoch}"
  echo pkgdesc:"${pkgdesc}"
  echo url:"${url}"
  echo install:"${install}"
  echo changelog:"${changelog}"
  license=("${license[@]//
/ }")
  printf 'license:%s\n' "${license[@]}"
  printf 'validpgpkeys:%s\n' "${validpgpkeys[@]}"
  printf 'noextract:%s\n' "${noextract[@]}"
  printf 'groups:%s\n' "${groups[@]}"
  printf 'backup:%s\n' "${backup[@]}"
  printf 'options:%s\n' "${options[@]}"
  if [[ $(type -t pkgver) == function ]]; then
    echo pkgver_func:y
  else
    echo pkgver_func:n
  fi
  echo ARCH
  echo arch:any
  printf 'source:%s\n' "${source[@]}"
  printf 'cksums:%s\n' "${cksums[@]}"
  printf 'md5sums:%s\n' "${md5sums[@]}"
  printf 'sha1sums:%s\n' "${sha1sums[@]}"
  printf 'sha224sums:%s\n' "${sha224sums[@]}"
  printf 'sha256sums:%s\n' "${sha256sums[@]}"
  printf 'sha384sums:%s\n' "${sha384sums[@]}"
  printf 'sha512sums:%s\n' "${sha512sums[@]}"
  printf 'b2sums:%s\n' "${b2sums[@]}"
  printf 'depends:%s\n' "${depends[@]}"
  printf 'makedepends:%s\n' "${makedepends[@]}"
  printf 'checkdepends:%s\n' "${checkdepends[@]}"
  printf 'optdepends:%s\n' "${optdepends[@]}"
  printf 'conflicts:%s\n' "${conflicts[@]}"
  printf 'provides:%s\n' "${provides[@]}"
  printf 'replaces:%s\n' "${replaces[@]}"
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
      declare -n _arch_source=source_"${_arch}" _arch_cksums=cksums_"${_arch}" _arch_md5sums=md5sums_"${_arch}" _arch_sha1sums=sha1sums_"${_arch}" _arch_sha224sums=sha224sums_"${_arch}" _arch_sha256sums=sha256sums_"${_arch}" _arch_sha384sums=sha384sums_"${_arch}" _arch_sha512sums=sha512sums_"${_arch}" _arch_b2sums=b2sums_"${_arch}" _arch_depends=depends_"${_arch}" _arch_makedepends=makedepends_"${_arch}" _arch_checkdepends=checkdepends_"${_arch}" _arch_optdepends=optdepends_"${_arch}" _arch_conflicts=conflicts_"${_arch}" _arch_provides=provides_"${_arch}" _arch_replaces=replaces_"${_arch}"
      printf 'source:%s\n' "${_arch_source[@]}"
      printf 'cksums:%s\n' "${_arch_cksums[@]}"
      printf 'md5sums:%s\n' "${_arch_md5sums[@]}"
      printf 'sha1sums:%s\n' "${_arch_sha1sums[@]}"
      printf 'sha224sums:%s\n' "${_arch_sha224sums[@]}"
      printf 'sha256sums:%s\n' "${_arch_sha256sums[@]}"
      printf 'sha384sums:%s\n' "${_arch_sha384sums[@]}"
      printf 'sha512sums:%s\n' "${_arch_sha512sums[@]}"
      printf 'b2sums:%s\n' "${_arch_b2sums[@]}"
      printf 'depends:%s\n' "${_arch_depends[@]}"
      printf 'makedepends:%s\n' "${_arch_makedepends[@]}"
      printf 'checkdepends:%s\n' "${_arch_checkdepends[@]}"
      printf 'optdepends:%s\n' "${_arch_optdepends[@]}"
      printf 'conflicts:%s\n' "${_arch_conflicts[@]}"
      printf 'provides:%s\n' "${_arch_provides[@]}"
      printf 'replaces:%s\n' "${_arch_replaces[@]}"
      unset -v checkdepends_"${_arch}" depends_"${_arch}" optdepends_"${_arch}" provides_"${_arch}" conflicts_"${_arch}" replaces_"${_arch}"
      echo END
    done
  fi
  _name_collapsed="${pkgname[*]}"
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
      _pkg_used='y'
    elif [[ "${_pkgname}" == "${pkgbase}" ]]; then
      echo END
      exit
    else
      echo "No package split function for ${_pkgname}"
      exit 4
    fi
    _arch_backup=("${arch[@]}")
    IFS=$'\n'
    _lines=($(declare -f "${_pkg_func}"))
    IFS="${_ifs_stored}"
    _buffer=
    for _line in "${_lines[@]:2:$((${#_lines[@]}-3))}"; do 
      if [[ "${_buffer}" ]]; then
        _buffer+="
        ${_line}"
        if [[ "${_line}" == *');' ]]; then
          eval "${_buffer}"
          _buffer=
        fi
      else
        [[ "${_line}" != *=* ]] && continue
        _line_value="${_line#*=}"
        _line_key="${_line%%=*}"
        _line="${_line_key##* }=${_line_value}"
        case "${_line}" in
          pkgdesc*)
            eval "${_line}"
            _pkg_pkgdesc='y'
            ;;
          url*)
            eval "${_line}"
            _pkg_url='y'
            ;;
          install*)
            eval "${_line}"
            _pkg_install='y'
            ;;
          changelog*)
            eval "${_line}"
            _pkg_changelog='y'
            ;;
          arch*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_arch='y'
            ;;
          license*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_license='y'
            ;;
          groups*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_groups='y'
            ;;
          backup*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_backup='y'
            ;;
          options*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_options='y'
            ;;
          checkdepends*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_checkdepends='y'
            ;;
          depends*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_depends='y'
            ;;
          optdepends*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_optdepends='y'
            ;;
          provides*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_provides='y'
            ;;
          conflicts*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_conflicts='y'
            ;;
          replaces*)
            if [[ "${_line}" == *');' ]]; then
              eval "${_line}"
            else
              _buffer="${_line}"
            fi
            _pkg_replaces='y'
            ;;
        esac
      fi
    done
    license=("${license[@]//
/ }")
    [[ "${_pkg_pkgdesc}" ]] && echo pkgdesc:"${pkgdesc}"
    [[ "${_pkg_url}" ]] && echo url:"${url}"
    [[ "${_pkg_install}" ]] && echo install:"${install}"
    [[ "${_pkg_changelog}" ]] && echo changelog:"${changelog}"
    [[ "${_pkg_license}" ]] && printf 'license:%s\n' "${license[@]}"
    [[ "${_pkg_groups}" ]] && printf 'groups:%s\n' "${groups[@]}"
    [[ "${_pkg_backup}" ]] && printf 'backup:%s\n' "${backup[@]}"
    [[ "${_pkg_options}" ]] && printf 'options:%s\n' "${options[@]}"
    echo PACKAGEARCH
    echo arch:any
    [[ "${_pkg_checkdepends}" ]] && printf 'checkdepends:%s\n' "${checkdepends[@]}"
    [[ "${_pkg_depends}" ]] && printf 'depends:%s\n' "${depends[@]}"
    [[ "${_pkg_optdepends}" ]] && printf 'optdepends:%s\n' "${optdepends[@]}"
    [[ "${_pkg_provides}" ]] && printf 'provides:%s\n' "${provides[@]}"
    [[ "${_pkg_conflicts}" ]] && printf 'conflicts:%s\n' "${conflicts[@]}"
    [[ "${_pkg_replaces}" ]] && printf 'replaces:%s\n' "${replaces[@]}"
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
        declare -n _arch_checkdepends=checkdepends_"${_arch}" _arch_depends=depends_"${_arch}" _arch_optdepends=optdepends_"${_arch}" _arch_provides=provides_"${_arch}" _arch_conflicts=conflicts_"${_arch}" _arch_replaces=replaces_"${_arch}"
        printf 'checkdepends:%s\n' "${_arch_checkdepends[@]}"
        printf 'depends:%s\n' "${_arch_depends[@]}"
        printf 'optdepends:%s\n' "${_arch_optdepends[@]}"
        printf 'provides:%s\n' "${_arch_provides[@]}"
        printf 'conflicts:%s\n' "${_arch_conflicts[@]}"
        printf 'replaces:%s\n' "${_arch_replaces[@]}"
        echo END
      done
    fi
    if [[ "${arch[*]}" != "${_arch_collapsed}" ]]; then
      arch=("${_arch_backup[@]}")
    fi
    echo END
  )
  done
  echo END
)
done
