extract_package_vars() { #1 suffix
  local ifs_stored="${IFS}"
  IFS=$'\n'
  local lines=($(declare -f package"$1"))
  IFS="${ifs_stored}"
  local buffer=
  for line in "${lines[@]:2:$((${#lines[@]}-3))}"; do 
    if [[ "${buffer}" ]]; then
      buffer+="
      ${line}"
      if [[ "${line}" == *');' ]]; then
        eval "${buffer}"
        buffer=
      fi
    elif [[ "${line}" =~ (depends|provides)'=('* ]]; then
      if [[ "${line}" == *');' ]]; then
        eval "${line}"
      else
        buffer="${line}"
      fi
    fi
  done
}
