)'=' ]]; then
        if [[ "${_line}" == *';' ]]; then
          eval "${_line}"
        else
          echo 'Unfinished package value line'
          exit 5
        fi
      elif [[ "${_line}" =~ ((arch