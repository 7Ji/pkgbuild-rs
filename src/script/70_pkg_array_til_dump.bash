)(|_.+))'=('* ]]; then
        if [[ "${_line}" == *');' ]]; then
          eval _pkg_"${_line}"
        else
          _buffer="${_line}"
        fi
      fi
    done
