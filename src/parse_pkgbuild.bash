source "${LIBRARY}/"util.sh
source "${LIBRARY}/"source.sh
source_makepkg_config
dump_array() { #1: var name, 2: report name
  declare -n array="$1"
  local item
  for item in "${array[@]}"; do
    echo "$2:${item}"
  done
}
dump_array_with_optional_arch() { #1: var name, 2: report name
  declare -n array="$1"
  declare -n array_arch="$1_${CARCH}"
  local item
  for item in "${array[@]}" "${array_arch[@]}"; do
    echo "$2:${item}"
  done
}
extract_package_vars() { #1 suffix
  local ifs_stored="${IFS}"
  IFS=$'\n'
  local lines=($(declare -f package"$1"))
  IFS="${ifs_stored}"
  for line in "${lines[@]:2:$((${#lines[@]}-3))}"; do 
    if [[ "${line}" =~ (depends|provides)'=('* ]]; then
      eval "${line}"
    fi
  done
}
while read -r line; do
  source "${line}"
  echo "[PKGBUILD]"
  pkgbase="${pkgbase:-${pkgname}}"
  echo "base:${pkgbase}"
  for item in "${pkgname[@]}"; do
    echo "name:${item}"
  done
  echo "ver:${pkgver}"
  echo "rel:${pkgrel}"
  echo "epoch:${epoch}"
  dump_array_with_optional_arch depends dep
  dump_array_with_optional_arch makedepends makedep
  dump_array_with_optional_arch provides provide
  dump_array_with_optional_arch source source
  for integ in {ck,md5,sha{1,224,256,384,512},b2}; do
    dump_array_with_optional_arch "${integ}"sums "${integ}"
  done
  echo -n "pkgver_func:"
  if [[ $(type -t pkgver) == 'function' ]]; then echo y; else echo n; fi
  unset -f pkgver package
  unset -v {depends,provides}{,_"${CARCH}"}
  extract_package_vars 
  dump_array_with_optional_arch depends dep_"${pkgbase}"
  dump_array_with_optional_arch provides provide_"${pkgbase}"
  for item in "${pkgname[@]}"; do
    unset -v {depends,provides}{,_"${CARCH}"}
    extract_package_vars _"${item}"
    dump_array_with_optional_arch depends dep_"${item}"
    dump_array_with_optional_arch provides provide_"${item}"
  done
  unset -v pkgbase pkgname {depends,makedepends,provides,source}{,_"${CARCH}"} \
           {ck,md5,sha{1,224,256,384,512},b2}sums{,_"${CARCH}"} \
           pkgver pkgrel epoch
done