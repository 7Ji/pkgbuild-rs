LIBRARY='/usr/share/makepkg'
MAKEPKG_CONF='/etc/makepkg.conf'
source '/usr/share/makepkg/util.sh'
source '/usr/share/makepkg/source.sh'
source_makepkg_config
_ifs_stored="${IFS}"
while read -r _line; do
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
      unset -v source_"${_arch}" cksums_"${_arch}" md5sums_"${_arch}" sha1sums_"${_arch}" sha224sums_"${_arch}" sha256sums_"${_arch}" sha384sums_"${_arch}" sha512sums_"${_arch}" b2sums_"${_arch}" depends_"${_arch}" makedepends_"${_arch}" checkdepends_"${_arch}" optdepends_"${_arch}" conflicts_"${_arch}" provides_"${_arch}" replaces_"${_arch}"
      echo END
    done
  fi
  _name_collapsed="${pkgname[*]}"
  _pkg_used=''
  for _pkgname in "${pkgname[@]}"; do
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
      continue
    else
      echo "No package split function for ${_pkgname}"
      exit 4
    fi
    unset -v pkgdesc url install changelog license groups backup options checkdepends depends optdepends provides conflicts replaces
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
      elif [[ "${_line}" =~ (pkgdesc|url|install|changelog)'+'?'=' ]]; then
        eval "${_line}"
      elif [[ "${_line}" =~ ((arch|license|groups|backup|options)|(checkdepends|depends|optdepends|provides|conflicts|replaces)(|_.+))'=('* ]]; then
        if [[ "${_line}" == *');' ]]; then
          eval "${_line}"
        else
          _buffer="${_line}"
        fi
      fi
    done
    echo pkgdesc:"${pkgdesc}"
    echo url:"${url}"
    echo install:"${install}"
    echo changelog:"${changelog}"
    license=("${license[@]//
/ }")
    printf 'license:%s\n' "${license[@]}"
    printf 'groups:%s\n' "${groups[@]}"
    printf 'backup:%s\n' "${backup[@]}"
    printf 'options:%s\n' "${options[@]}"
    echo PACKAGEARCH
    echo arch:any
    printf 'checkdepends:%s\n' "${checkdepends[@]}"
    printf 'depends:%s\n' "${depends[@]}"
    printf 'optdepends:%s\n' "${optdepends[@]}"
    printf 'provides:%s\n' "${provides[@]}"
    printf 'conflicts:%s\n' "${conflicts[@]}"
    printf 'replaces:%s\n' "${replaces[@]}"
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
        unset -v source_"${_arch}" cksums_"${_arch}" md5sums_"${_arch}" sha1sums_"${_arch}" sha224sums_"${_arch}" sha256sums_"${_arch}" sha384sums_"${_arch}" sha512sums_"${_arch}" b2sums_"${_arch}" depends_"${_arch}" makedepends_"${_arch}" checkdepends_"${_arch}" optdepends_"${_arch}" conflicts_"${_arch}" provides_"${_arch}" replaces_"${_arch}"
        echo END
      done
    fi
    if [[ "${arch[*]}" != "${_arch_collapsed}" ]]; then
      arch=("${_arch_backup[@]}")
    fi
    echo END
  done
  unset -f pkgver package{,_"${pkgbase}"} "${pkgname[@]/#/package_}"
  unset -v pkgname arch pkgbase pkgver pkgrel epoch pkgdesc url install changelog license validpgpkeys noextract groups backup options source cksums md5sums sha1sums sha224sums sha256sums sha384sums sha512sums b2sums depends makedepends checkdepends optdepends conflicts provides replaces
  echo END
done
