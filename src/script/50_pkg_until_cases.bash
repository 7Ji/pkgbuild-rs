    _arch_backup=("${arch[@]}")
    IFS=$'\n'
    _lines=($(declare -f "${_pkg_func}"))
    IFS="${_ifs_stored}"
    _buffer=
    for _line in "${_lines[@]:2:$((${#_lines[@]}-3))}"; do 
      if [[ "${_buffer}" ]]; then
        _buffer+="
        ${_line}"
        if [[ "${_line}" == *');' || "${_line}" == *')' ]]; then
          eval "${_buffer}"
          _buffer=
        fi
      else
        [[ "${_line}" != *=* ]] && continue
        _line_value="${_line#*=}"
        _line_key="${_line%%=*}"
        _line="${_line_key##* }=${_line_value}"
        case "${_line}" in
