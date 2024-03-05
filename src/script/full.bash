source "${LIBRARY}/"util.sh
source "${LIBRARY}/"source.sh
source_makepkg_config
# dump_array() { #1: var name, 2: report name
#   declare -n array="$1"
#   local item
#   for item in "${array[@]}"; do
#     echo "$2:${item}"
#   done
# }
# dump_array_with_optional_arch() { #1: var name, 2: report name
#   declare -n array="$1"
#   declare -n array_arch="$1_${CARCH}"
#   local item
#   for item in "${array[@]}" "${array_arch[@]}"; do
#     echo "$2:${item}"
#   done
# }
# extract_package_vars() { #1 suffix
#   local ifs_stored="${IFS}"
#   IFS=$'\n'
#   local lines=($(declare -f package"$1"))
#   IFS="${ifs_stored}"
#   local buffer=
#   for line in "${lines[@]:2:$((${#lines[@]}-3))}"; do 
#     if [[ "${buffer}" ]]; then
#       buffer+="
#       ${line}"
#       if [[ "${line}" == *');' ]]; then
#         eval "${buffer}"
#         buffer=
#       fi
#     elif [[ "${line}" =~ (depends|provides)'=('* ]]; then
#       if [[ "${line}" == *');' ]]; then
#         eval "${line}"
#       else
#         buffer="${line}"
#       fi
#     fi
#   done
# }

_pkgbuild_array_items=(
  license validgpgkeys noextract groups backup options
)
_pkg_array_items=(
  license groups backup options
)
_arch_specific_items=(
  {source,{ck,md5,sha{1,224,256,384,512},b2}sum,{,make,check,opt}depend,conflict,provide,replace}s
)
_pkg_arch_specific_items=(
  {depend,optdepend,provide,conflict,replace}s
)
while read -r line; do
  source "${line}"
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
  for _items in  "${_pkgbuild_array_items[@]}"; do
    declare -n _pkgbuild_items="${_items}"
    for _item in "${_pkgbuild_items[@]}"; do
      echo "${_items}:${_item}"
    done
  done
  if [[ $(type -t package) == function ]]; then
    echo pkgver_func:y
  else
    echo pkgver_func:n
  fi
  echo ARCH
  echo arch:any
  for _items in "${_arch_specific_items[@]}"; do
    declare -n _arch_items="${_items}"
    for _item in "${_arch_items[@]}"; do
      echo "${_items}:${_item}"
    done
    unset -v "${_items}"
  done
  echo END
  _arch_collapsed="${arch[*]}"
  if [[ " ${_arch_collapsed} " =~ ' any ' ]]; then
    if [[ "${#_arch_collapsed}" != 5 ]]; then
      echo "ERROR: Architecture 'any' found when multiple architecture defined"
      exit 1
    fi
  else
    for _arch in "${arch[@]}"; do
      echo ARCH
      echo arch:"${_arch}"
      for _items in "${_arch_specific_items[@]}"; do
        _items_name="${_items}_${_arch}"
        declare -n _arch_items="${_items_name}"
        for _item in "${_arch_items[@]}"; do
          echo "${_items}:${_item}"
        done
        unset -v "${_items_name}"
      done
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
        exit 1
      fi
      _pkg_func=package
      _pkg_used='y'
    elif [[ "${_pkgname}" == "${pkgbase}" ]]; then
      echo END
      continue
    else
      echo "No package split function for ${_pkgname}"
      exit 1
    fi
    unset -v pkgdesc url license groups backup options install changelog
    _ifs_stored="${IFS}"
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
      elif [[ "${_line}" =~ (pkgdesc|url|install|changelog)'="' ]]; then
        if [[ "${_line}" == *'";' ]]; then
          eval "${_line}"
        else
          echo 'Unfinished package value line'
          exit 1
        fi
      elif [[ "${_line}" =~ (license|groups|backup|options|depends|optdepends|provides|conflicts|replaces)'=('* ]]; then
        if [[ "${_line}" == *');' ]]; then
          eval "${_line}"
        else
          buffer="${_line}"
        fi
      fi
    done
    echo pkgdesc:"${pkgdesc}"
    echo url:"${url}"
    echo install:"${install}"
    echo changelog:"${changelog}"
    for _items in  "${_pkg_array_items[@]}"; do
      declare -n _pkg_items="${_items}"
      for _item in "${_pkg_items[@]}"; do
        echo "${_items}:${_item}"
      done
    done
    echo PACKAGEARCH
    echo arch:any
    for _items in "${_pkg_arch_specific_items[@]}"; do
      declare -n _arch_items="${_items}"
      for _item in "${_arch_items[@]}"; do
        echo "${_items}:${_item}"
      done
      unset -v "${_items}"
    done
    echo END
    if [[ " ${_arch_collapsed} " =~ ' any ' ]]; then
      if [[ "${#_arch_collapsed}" != 5 ]]; then
        echo "ERROR: Architecture 'any' found when multiple architecture defined"
        exit 1
      fi
    else
      for _arch in "${arch[@]}"; do
        echo PACKAGEARCH
        echo arch:"${_arch}"
        for _items in "${_pkg_arch_specific_items[@]}"; do
          _items_name="${_items}_${_arch}"
          declare -n _arch_items="${_items_name}"
          for _item in "${_arch_items[@]}"; do
            echo "${_items}:${_item}"
          done
          unset -v "${_items_name}"
        done
        echo END
      done
    fi
    echo END
  done
  unset -v pkgdesc url license groups backup options install changelog
  unset -f package{,_"${pkgbase}"} "${pkgname[@]/#/package_}"
  echo END

  unset -v pkgbase pkgver pkgrel epoch pkgdesc url license install changelog \
    validgpgkeys noextract groups backups options
done
