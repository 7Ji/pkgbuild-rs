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
